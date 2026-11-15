//! Optional desktop delivery. Call from a background executor; delivery errors are returned.
//! Linux notification bodies are markup: escape untrusted text as in Switchboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}
#[derive(Debug, Clone)]
pub struct DesktopNotification {
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub icon: String,
    pub urgency: Urgency,
    pub timeout_ms: i32,
}
impl DesktopNotification {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            app_name: "GUIraffe".into(),
            title: title.into(),
            body: body.into(),
            icon: String::new(),
            urgency: Urgency::Normal,
            timeout_ms: 10_000,
        }
    }
    /// Blocking platform delivery; callers should run it on their background executor.
    #[cfg(not(any(target_os = "ios", target_os = "android", target_family = "wasm")))]
    pub fn show(&self) -> Result<(), String> {
        #[cfg(all(unix, not(target_os = "macos")))]
        let body = escape_markup(&self.body);
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        let body = self.body.clone();
        let mut notification = notify_rust::Notification::new();
        notification
            .appname(&self.app_name)
            .summary(&self.title)
            .body(&body)
            .icon(&self.icon)
            .timeout(self.timeout_ms);
        #[cfg(target_os = "linux")]
        notification.urgency(match self.urgency {
            Urgency::Low => notify_rust::Urgency::Low,
            Urgency::Normal => notify_rust::Urgency::Normal,
            Urgency::Critical => notify_rust::Urgency::Critical,
        });
        notification.show().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(any(target_os = "ios", target_os = "android", target_family = "wasm"))]
    pub fn show(&self) -> Result<(), String> {
        Err(
            "Desktop notifications are unavailable on this platform; use OverlayHost::notify"
                .into(),
        )
    }
}
pub fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_body_is_plain_text() {
        assert_eq!(
            escape_markup("<b>A & B</b>"),
            "&lt;b&gt;A &amp; B&lt;/b&gt;"
        );
    }
    #[test]
    fn defaults() {
        let n = DesktopNotification::new("Hello", "Body");
        assert_eq!(n.urgency, Urgency::Normal);
        assert_eq!(n.timeout_ms, 10_000);
    }
}
