use bevy_ecs::{entity::Entity, prelude::*};

use crate::{
    ProjectionCtx, StyleClass, UiLabel, UiView, controls::UiControlTemplate,
    templates::ensure_template_part,
};

/// Modal dialog entity projected in the overlay layer.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct UiDialog {
    pub title: String,
    pub body: String,
    pub dismiss_label: String,
    pub title_key: Option<String>,
    pub body_key: Option<String>,
    pub dismiss_key: Option<String>,
}

impl UiDialog {
    #[must_use]
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            dismiss_label: "Close".to_string(),
            title_key: None,
            body_key: None,
            dismiss_key: None,
        }
    }

    #[must_use]
    pub fn with_localized_keys(
        mut self,
        title_key: impl Into<String>,
        body_key: impl Into<String>,
        dismiss_key: impl Into<String>,
    ) -> Self {
        self.title_key = Some(title_key.into());
        self.body_key = Some(body_key.into());
        self.dismiss_key = Some(dismiss_key.into());
        self
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartDialogTitle;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartDialogBody;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartDialogDismiss;

impl UiControlTemplate for UiDialog {
    fn expand(world: &mut World, entity: Entity) {
        let dialog = world.get::<UiDialog>(entity).cloned();
        let Some(dialog) = dialog else {
            return;
        };

        let title_part = ensure_template_part::<PartDialogTitle, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["overlay.dialog.title".to_string()]),
            )
        });
        let body_part = ensure_template_part::<PartDialogBody, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["overlay.dialog.body".to_string()]),
            )
        });
        let dismiss_part = ensure_template_part::<PartDialogDismiss, _>(world, entity, || {
            (
                UiLabel::new(""),
                StyleClass(vec!["overlay.dialog.dismiss".to_string()]),
            )
        });

        if let Some(mut label) = world.get_mut::<UiLabel>(title_part) {
            label.text = dialog.title;
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(body_part) {
            label.text = dialog.body;
        }
        if let Some(mut label) = world.get_mut::<UiLabel>(dismiss_part) {
            label.text = dialog.dismiss_label;
        }
    }

    fn project(component: &Self, ctx: ProjectionCtx<'_>) -> UiView {
        crate::projection::dialog::project_dialog(component, ctx)
    }

    fn default_style_ron() -> &'static str {
        r##"(
  rules: [
    (
      selector: Class("overlay.modal.dimmer"),
      setter: (
        colors: (
          bg: Rgba8(0, 0, 0, 160),
        ),
      ),
    ),
    (
      selector: Type("UiDialog"),
      setter: (
        layout: (
          padding: 18.0,
          gap: 10.0,
          corner_radius: 12.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#181E2D"),
          border: Hex("#3A4868"),
        ),
      ),
    ),
    (
      selector: Class("overlay.dialog.title"),
      setter: (
        text: (
          size: 24.0,
        ),
        colors: (
          text: Hex("#F1F5FF"),
        ),
      ),
    ),
    (
      selector: Class("overlay.dialog.body"),
      setter: (
        text: (
          size: 16.0,
        ),
        colors: (
          text: Hex("#C6D2EE"),
        ),
      ),
    ),
    (
      selector: Class("overlay.dialog.dismiss"),
      setter: (
        layout: (
          padding: 8.0,
          corner_radius: 8.0,
          border_width: 1.0,
        ),
        colors: (
          bg: Hex("#273652"),
          hover_bg: Hex("#31466B"),
          pressed_bg: Hex("#1E2D47"),
          border: Hex("#4E6697"),
          text: Hex("#E3ECFF"),
        ),
        text: (
          size: 15.0,
        ),
      ),
    ),
  ],
)
"##
    }
}
