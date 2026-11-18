use gpui_kit::{App, EntityId, Window};
use guiraffe_motion::{Animated, MotionPrefs};

/// Frames must explicitly invalidate the control that owns the spring. The backend's
/// current view can refer to the host while the control's element tree is built.
pub(crate) fn progress(spring: &mut Animated<f32>, needs_frame: &mut bool, cx: &App) -> f32 {
    let (value, moving) = spring.value_at(cx.background_executor().now(), &MotionPrefs::global(cx));
    *needs_frame |= moving;
    if !moving {
        spring.jump(value);
    }
    value
}

pub(crate) fn schedule(needs_frame: bool, entity: EntityId, window: &Window) {
    if needs_frame {
        window.on_next_frame(move |_, cx| cx.notify(entity));
    }
}
