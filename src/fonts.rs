//! Bundled fonts (OFL-1.1, licence next to the files in
//! `assets/fonts/`). The widget loads them into iced's font system once.

/// The code font's family name.
pub const MONO: &str = "JetBrains Mono";

/// JetBrains Mono 2.304 regular, italic and bold, unmodified from
/// <https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304>.
pub const JETBRAINS_MONO: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/JetBrainsMono/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono/JetBrainsMono-Bold.ttf"),
];
