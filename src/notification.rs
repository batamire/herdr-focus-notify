#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusNotification {
    pub(crate) pane_id: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// Secondary line shown under the message (alerter `--subtitle`): where the
    /// pane works and which pane it is. None until enrichment reads the pane.
    pub(crate) subtitle: Option<String>,
    pub(crate) group: String,
    pub(crate) app_icon: Option<String>,
}
