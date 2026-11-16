pragma Singleton
import QtQuick

// Colors and corners, following the Omarchy theme live. The values below are
// what shows before the theme arrives, or when Omarchy isn't there at all.
QtObject {
    property color background: "#16171b"
    property color sidebar: "#111215"
    property color surface: "#1f2126"
    property color raised: "#2a2d34"
    property color foreground: "#e6e8ec"
    property color secondary: "#a4a9b3"
    property color tertiary: "#6c7280"
    property color accent: "#9ece6a"
    property color red: "#e5676b"
    property color green: "#7fc27a"
    property color yellow: "#e6c36a"
    property int radius: 0

    // Black or white, whichever reads on the accent (WCAG relative luminance).
    readonly property color onAccent: {
        const lin = v => v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
        const l = 0.2126 * lin(accent.r) + 0.7152 * lin(accent.g) + 0.0722 * lin(accent.b)
        return l > 0.179 ? "#000000" : "#ffffff"
    }
    readonly property color line: Qt.rgba(foreground.r, foreground.g, foreground.b, 0.08)
    readonly property color hover: Qt.rgba(foreground.r, foreground.g, foreground.b, 0.06)
    readonly property color pressed: Qt.rgba(foreground.r, foreground.g, foreground.b, 0.11)
    readonly property color accentSoft: Qt.rgba(accent.r, accent.g, accent.b, 0.16)

    // Small things never round more than a little, however round the windows.
    readonly property int small: Math.min(6, radius)
    // Cards round a little more than buttons, still capped well below windows.
    readonly property int card: Math.min(10, radius)
    readonly property string font: "Adwaita Sans"
    readonly property int quick: 140
    readonly property int smooth: 240

    // Type scale. Pages use these, not literal sizes.
    readonly property int display: 26
    readonly property int title: 18
    readonly property int heading: 15
    readonly property int body: 14
    readonly property int label: 13
    readonly property int caption: 12

    // Spacing steps.
    readonly property int s1: 4
    readonly property int s2: 8
    readonly property int s3: 12
    readonly property int s4: 16
    readonly property int s5: 24
    readonly property int s6: 32

    // Elevation: page → card → something floating over cards.
    readonly property color overlay: Qt.lighter(surface, 1.18)
    readonly property color scrim: Qt.rgba(0, 0, 0, 0.55)
    readonly property color accentLine: Qt.rgba(accent.r, accent.g, accent.b, 0.55)

    // How wide a page's content may grow, and its side margin, for a width.
    function margin(width) { return width >= 1400 ? 40 : width >= 900 ? 28 : 18 }

    function apply(t) {
        if (!t || !t.background)
            return
        background = t.background
        sidebar = t.dark_background || Qt.darker(t.background, 1.18)
        surface = t.lighter_background || Qt.lighter(t.background, 1.25)
        raised = t.selection || Qt.lighter(t.background, 1.5)
        foreground = t.foreground || foreground
        secondary = t.light_foreground || Qt.darker(foreground, 1.3)
        // The theme's dimmest text color is tuned for terminal comments and
        // is too faint for interface labels, so blend toward the secondary.
        tertiary = t.dark_foreground ? Qt.tint(t.dark_foreground, Qt.rgba(secondary.r, secondary.g, secondary.b, 0.35)) : tertiary
        accent = t.accent || accent
        red = t.red || red
        green = t.green || green
        yellow = t.yellow || yellow
        radius = t.radius || 0
    }
}
