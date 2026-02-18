use std::{
    collections::{HashSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io,
    path::Path,
    sync::Arc,
};

use bevy_asset::{AssetEvent, Assets};
use bevy_ecs::{message::MessageReader, prelude::*, system::NonSendMut};
use bevy_text::Font;
use masonry::peniko::Blob;

use crate::MasonryRuntime;

/// Font bridge resource that stores pending font files for registration in Masonry/Parley.
///
/// Fonts can be queued either from raw bytes or by file path (for example
/// `assets/fonts/NotoSansCJK-Regular.otf`).
#[derive(Resource, Debug, Default)]
pub struct XilemFontBridge {
    pending_fonts: Vec<Vec<u8>>,
    registered_fingerprints: HashSet<u64>,
}

impl XilemFontBridge {
    /// Queue a font from raw bytes. Returns `true` when queued, `false` if duplicate.
    pub fn register_font_bytes(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return false;
        }

        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let fingerprint = hasher.finish();

        if !self.registered_fingerprints.insert(fingerprint) {
            return false;
        }

        self.pending_fonts.push(bytes.to_vec());
        true
    }

    /// Queue a font by reading it from disk.
    ///
    /// Typical path for Bevy projects: `assets/fonts/<font-file>.ttf|otf`.
    pub fn register_font_path(&mut self, path: impl AsRef<Path>) -> io::Result<bool> {
        let data = fs::read(path)?;
        Ok(self.register_font_bytes(&data))
    }

    pub fn take_pending_fonts(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_fonts)
    }
}

/// Option A bridge: consume Bevy `AssetEvent<Font>` and queue loaded font bytes.
///
/// This enables dynamic loading via `AssetServer::load("fonts/...")`.
pub fn collect_bevy_font_assets(
    mut font_events: MessageReader<AssetEvent<Font>>,
    fonts: Option<Res<Assets<Font>>>,
    mut bridge: ResMut<XilemFontBridge>,
) {
    let Some(fonts) = fonts else {
        return;
    };

    for event in font_events.read() {
        let Some(id) = (match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            AssetEvent::Removed { .. } | AssetEvent::Unused { .. } => None,
        }) else {
            continue;
        };

        if let Some(font) = fonts.get(id) {
            bridge.register_font_bytes(font.data.as_slice());
        }
    }
}

/// Sync pending font bytes into Masonry's internal text/font database.
///
/// This is the bridge between Bevy-side app setup and Xilem/Masonry font shaping.
pub fn sync_fonts_to_xilem(
    mut runtime: NonSendMut<MasonryRuntime>,
    mut bridge: ResMut<XilemFontBridge>,
) {
    let pending = bridge.take_pending_fonts();
    if pending.is_empty() {
        return;
    }

    for font_bytes in pending {
        runtime
            .render_root
            .register_fonts(Blob::new(Arc::new(font_bytes)));
    }
}
