//! Bundled fonts (OFL-1.1, licence next to the files in
//! `assets/fonts/`). The widget loads them into iced's font system once.

/// The prose font's family name (the user's pick, 2026-10-02).
pub const PROSE: &str = "Atkinson Hyperlegible Next";

/// Atkinson Hyperlegible Next 2.001 regular, italic, bold and bold italic,
/// unmodified from <https://github.com/googlefonts/atkinson-hyperlegible-next>.
pub const ATKINSON_HYPERLEGIBLE_NEXT: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext/AtkinsonHyperlegibleNext-Regular.ttf"),
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext/AtkinsonHyperlegibleNext-Italic.ttf"),
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext/AtkinsonHyperlegibleNext-Bold.ttf"),
    include_bytes!(
        "../assets/fonts/AtkinsonHyperlegibleNext/AtkinsonHyperlegibleNext-BoldItalic.ttf"
    ),
];

/// The code font's family name.
pub const MONO: &str = "JetBrains Mono";

/// JetBrains Mono 2.304 regular, italic and bold, unmodified from
/// <https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304>.
pub const JETBRAINS_MONO: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/JetBrainsMono/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono/JetBrainsMono-Bold.ttf"),
];
