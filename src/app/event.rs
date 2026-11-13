/// An event that travels between the parts of the running application.
///
/// The dashboard reads the host state from `AppState`, so this carries the
/// things that are not state: the request to shut down.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Shutdown,
}
