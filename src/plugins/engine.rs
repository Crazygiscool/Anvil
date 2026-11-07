use crate::discovery::model::Diagnostic;
use crate::platform::TextReader;
use crate::plugins::model::{PLUGIN_API_VERSION, PluginCatalog, PluginManifest, PluginReport};
use std::path::{Path, PathBuf};
use wasmtime::{Engine, Module, Store};

pub trait PluginBackend: Send + Sync {
    fn scan(&self, reader: &dyn TextReader, directories: &[PathBuf]) -> PluginCatalog;
}

pub struct WasmPluginEngine;

impl WasmPluginEngine {
    pub fn new() -> Self {
        Self
    }
}

impl PluginBackend for WasmPluginEngine {
    fn scan(&self, reader: &dyn TextReader, directories: &[PathBuf]) -> PluginCatalog {
        let mut catalog = PluginCatalog {
            api_version: PLUGIN_API_VERSION,
            plugins: Vec::new(),
            diagnostics: Vec::new(),
        };
        let engine = Engine::default();

        for directory in directories {
            let mut manifests: Vec<PathBuf> = Vec::new();
            match std::fs::read_dir(directory) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().and_then(|value| value.to_str()) == Some("toml")
                        {
                            manifests.push(path);
                        }
                    }
                    manifests.sort();
                }
                Err(error) => {
                    catalog.diagnostics.push(Diagnostic {
                        source: directory.display().to_string(),
                        error: error.to_string(),
                    });
                    continue;
                }
            }

            for manifest_path in manifests {
                let source = match reader.read_to_string(&manifest_path) {
                    Ok(source) => source,
                    Err(error) => {
                        catalog.diagnostics.push(Diagnostic {
                            source: manifest_path.display().to_string(),
                            error: error.to_string(),
                        });
                        continue;
                    }
                };

                let manifest: PluginManifest = match toml::from_str(&source) {
                    Ok(manifest) => manifest,
                    Err(error) => {
                        catalog.diagnostics.push(Diagnostic {
                            source: manifest_path.display().to_string(),
                            error: error.to_string(),
                        });
                        continue;
                    }
                };

                if manifest.api_version != PLUGIN_API_VERSION {
                    catalog.diagnostics.push(Diagnostic {
                        source: manifest_path.display().to_string(),
                        error: format!(
                            "plugin requires API {}, but this host provides {}",
                            manifest.api_version, PLUGIN_API_VERSION
                        ),
                    });
                    continue;
                }

                if let Some(reason) = unsafe_capability(manifest.capabilities.as_slice()) {
                    catalog.diagnostics.push(Diagnostic {
                        source: manifest_path.display().to_string(),
                        error: format!("plugin capability is not allowed: {reason}"),
                    });
                    continue;
                }

                let parent = manifest_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                if manifest.library.contains("..") || Path::new(&manifest.library).is_absolute() {
                    catalog.diagnostics.push(Diagnostic {
                        source: manifest_path.display().to_string(),
                        error: "plugin library path must be relative and safe".to_string(),
                    });
                    continue;
                }
                let library = parent.join(&manifest.library);
                if !reader.exists(&library) {
                    catalog.diagnostics.push(Diagnostic {
                        source: library.display().to_string(),
                        error: "plugin library does not exist".to_string(),
                    });
                    continue;
                }

                let bytes = match std::fs::read(&library) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        catalog.diagnostics.push(Diagnostic {
                            source: library.display().to_string(),
                            error: error.to_string(),
                        });
                        continue;
                    }
                };

                let module = match Module::new(&engine, &bytes) {
                    Ok(module) => module,
                    Err(error) => {
                        catalog.diagnostics.push(Diagnostic {
                            source: library.display().to_string(),
                            error: error.to_string(),
                        });
                        continue;
                    }
                };

                let exports: Vec<String> = module
                    .exports()
                    .map(|export| export.name().to_string())
                    .collect();
                let required_entry = manifest.entrypoint.clone();
                let entry_function_exists = module
                    .exports()
                    .any(|export| export.name() == required_entry && export.ty().func().is_some());
                let entry_result = if entry_function_exists {
                    let mut store = Store::new(&engine, ());
                    store.set_fuel(1_000_000).ok();
                    match wasmtime::Instance::new(&mut store, &module, &[]) {
                        Ok(instance) => {
                            if let Some(function) = instance.get_func(&mut store, &required_entry) {
                                let mut results = [wasmtime::Val::I64(0)];
                                match function.call(&mut store, &[], &mut results) {
                                    Ok(()) => match results[0] {
                                        wasmtime::Val::I64(value) => Some(value),
                                        _ => Some(0),
                                    },
                                    Err(error) => {
                                        catalog.diagnostics.push(Diagnostic {
                                            source: library.display().to_string(),
                                            error: format!("entrypoint failed: {error}"),
                                        });
                                        None
                                    }
                                }
                            } else {
                                None
                            }
                        }
                        Err(error) => {
                            catalog.diagnostics.push(Diagnostic {
                                source: library.display().to_string(),
                                error: format!("instantiate failed: {error}"),
                            });
                            None
                        }
                    }
                } else {
                    None
                };

                if !entry_function_exists {
                    catalog.diagnostics.push(Diagnostic {
                        source: library.display().to_string(),
                        error: format!("entrypoint {} is not exported", required_entry),
                    });
                }

                let invoked = entry_function_exists && entry_result.is_some();
                catalog.plugins.push(PluginReport {
                    name: manifest.name,
                    version: manifest.version,
                    api_version: manifest.api_version,
                    path: manifest_path,
                    library,
                    capabilities: manifest.capabilities,
                    exports,
                    invoked,
                    entry_result,
                    diagnostics: Vec::new(),
                });
            }
        }

        catalog
    }
}

fn unsafe_capability(capabilities: &[String]) -> Option<String> {
    const ALLOWED: &[&str] = &["discovery", "storage", "runtime", "telemetry", "read-only"];
    capabilities
        .iter()
        .find(|capability| !ALLOWED.contains(&capability.as_str()))
        .cloned()
}

pub fn default_plugin_directories(effective_uid: u32) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if effective_uid == 0 {
        directories.push(PathBuf::from("/etc/anvil/plugins"));
    } else {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            directories.push(PathBuf::from(xdg).join("anvil/plugins"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            directories.push(PathBuf::from(home).join(".config/anvil/plugins"));
        }
        directories.push(PathBuf::from("/etc/anvil/plugins"));
    }
    directories
}
