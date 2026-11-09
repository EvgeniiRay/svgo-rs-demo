//! Subsets of `plugins/_collections.js` needed by this group of plugins.

pub fn is_presentation(name: &str) -> bool {
    matches!(
        name,
        "alignment-baseline" | "baseline-shift" | "clip-path" | "clip-rule" | "clip"
            | "color-interpolation-filters" | "color-interpolation" | "color-profile"
            | "color-rendering" | "color" | "cursor" | "direction" | "display"
            | "dominant-baseline" | "enable-background" | "fill-opacity" | "fill-rule" | "fill"
            | "filter" | "flood-color" | "flood-opacity" | "font-family" | "font-size-adjust"
            | "font-size" | "font-stretch" | "font-style" | "font-variant" | "font-weight"
            | "glyph-orientation-horizontal" | "glyph-orientation-vertical" | "image-rendering"
            | "letter-spacing" | "lighting-color" | "marker-end" | "marker-mid" | "marker-start"
            | "mask" | "opacity" | "overflow" | "paint-order" | "pointer-events"
            | "shape-rendering" | "stop-color" | "stop-opacity" | "stroke-dasharray"
            | "stroke-dashoffset" | "stroke-linecap" | "stroke-linejoin" | "stroke-miterlimit"
            | "stroke-opacity" | "stroke-width" | "stroke" | "text-anchor" | "text-decoration"
            | "text-overflow" | "text-rendering" | "transform-origin" | "transform"
            | "unicode-bidi" | "vector-effect" | "visibility" | "word-spacing" | "writing-mode"
    )
}

pub fn is_non_rendering(name: &str) -> bool {
    matches!(
        name,
        "clipPath" | "filter" | "linearGradient" | "marker" | "mask" | "pattern"
            | "radialGradient" | "solidColor" | "symbol"
    )
}

pub fn is_references_prop(name: &str) -> bool {
    matches!(
        name,
        "clip-path" | "color-profile" | "fill" | "filter" | "marker-end" | "marker-mid"
            | "marker-start" | "mask" | "stroke" | "style"
    )
}

/// Pseudo-classes SVGO evaluates itself (functional + tree-structural).
pub fn is_preserved_pseudo(name: &str) -> bool {
    matches!(
        name,
        "is" | "not" | "where" | "has" | "empty" | "first-child" | "first-of-type"
            | "last-child" | "last-of-type" | "nth-child" | "nth-last-child"
            | "nth-last-of-type" | "nth-of-type" | "only-child" | "only-of-type" | "root"
    )
}
