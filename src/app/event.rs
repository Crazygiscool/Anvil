/// An event that travels between the parts of the running application.
///
/// The dashboard reads the host state from `AppState`, so this carries the
/// things that are not state: the request to shut down, and the notice
/// that something the view shows has changed.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Shutdown,
    /// The collected state changed and the view has to be redrawn.
    ///
    /// The event loop waits on this instead of redrawing on a timer, so the
    /// screen follows the host as soon as a report lands rather than at
    /// the next tick.
    StateChanged,
}
