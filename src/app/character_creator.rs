use std::collections::HashMap;

/// Describes the pixel bounds for a single sprite slice inside the character
/// creator atlas. This keeps the layout declarative so both the UI and any
/// export helpers can reuse the same source of truth when compositing the
/// figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteSlice {
    /// Left coordinate of the sprite within the atlas texture in pixels.
    pub x: u32,
    /// Top coordinate of the sprite within the atlas texture in pixels.
    pub y: u32,
    /// Width of the sprite portion in pixels.
    pub width: u32,
    /// Height of the sprite portion in pixels.
    pub height: u32,
}

impl SpriteSlice {
    /// Convenience constructor so callers can build slices in a single place
    /// without repeating the field names, keeping the atlas definition tidy.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
}

/// Exhaustive list of male body sprite pieces and their atlas coordinates.
/// The entries originate from the design sheet provided by the art team and
/// let the compositor stack each limb precisely without manual tweaking in the
/// UI code.
pub const MALE_BODY_SPRITES: &[(&str, SpriteSlice)] = &[
    ("HEAD", SpriteSlice::new(87, 20, 141, 208)),
    ("NECK", SpriteSlice::new(296, 165, 107, 275)),
    ("BODY", SpriteSlice::new(15, 400, 262, 411)),
    ("HIP", SpriteSlice::new(53, 829, 213, 167)),
    ("UPPER_ARM_R", SpriteSlice::new(428, 172, 101, 268)),
    ("LOWER_ARM_R", SpriteSlice::new(685, 232, 136, 321)),
    ("HAND_0_R", SpriteSlice::new(311, 677, 93, 135)),
    ("HAND_1_R", SpriteSlice::new(318, 847, 92, 155)),
    ("UPPER_ARM_L", SpriteSlice::new(844, 233, 131, 326)),
    ("LOWER_ARM_L", SpriteSlice::new(432, 468, 86, 189)),
    ("HAND_0_L", SpriteSlice::new(432, 690, 91, 122)),
    ("HAND_1_L", SpriteSlice::new(437, 850, 89, 145)),
    ("UPPER_LEG_R", SpriteSlice::new(696, 579, 107, 296)),
    ("LOWER_LEG_R", SpriteSlice::new(699, 582, 102, 292)),
    ("FOOT_R", SpriteSlice::new(678, 921, 163, 75)),
    ("UPPER_LEG_L", SpriteSlice::new(844, 580, 107, 296)),
    ("LOWER_LEG_L", SpriteSlice::new(847, 583, 102, 292)),
    ("FOOT_L", SpriteSlice::new(857, 924, 163, 75)),
    ("LOWER_LEG_F", SpriteSlice::new(551, 564, 107, 315)),
    ("FOOT_F", SpriteSlice::new(553, 910, 111, 87)),
];

/// Builds a hash map keyed by sprite identifier for quick lookup during
/// runtime. The helper keeps the UI code lean and avoids repeated iteration
/// every time the preview refreshes.
pub fn male_body_sprite_map() -> HashMap<&'static str, SpriteSlice> {
    MALE_BODY_SPRITES.iter().copied().collect()
}
