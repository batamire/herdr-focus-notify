#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusNotification {
    pub(crate) pane_id: String,
    pub(crate) status: String,
    /// Agent display name; the notification's subtitle, mirroring the stock
    /// Agent sidebar row that carries the agent alone.
    pub(crate) agent: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// Secondary line shown under the message (alerter `--subtitle`).
    pub(crate) subtitle: Option<String>,
    pub(crate) group: String,
    pub(crate) app_icon: Option<String>,
}
