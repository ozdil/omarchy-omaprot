pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    readonly property string homeDir: Quickshell.env("HOME")
    readonly property string omarchyStateDir: homeDir + "/.local/state/omarchy/current"

    property string themeName: "monotone"
    property bool isDarkTheme: true

    property color bgDark: "#0a0a0a"
    property color bgBase: "#111111"
    property color bgSurface: "#1a1a1a"
    property color bgCard: "#222222"
    property color bgCardHover: "#2c2c2c"
    property color border: "#333333"
    property color borderLight: "#444444"

    property color textMain: "#e5e5e5"
    property color textMuted: "#888888"
    property color textDim: "#555555"

    property color accent: "#b8b8b8"
    property color accentHover: "#cccccc"
    property color accentSuccess: "#a6da95"
    property color accentWarning: "#eed49f"
    property color accentDanger: "#ed8796"

    readonly property int radiusSm: 4
    readonly property int radiusMd: 8
    readonly property int radiusLg: 12

    readonly property string fontFamily: "JetBrainsMono Nerd Font, JetBrains Mono, monospace"
    readonly property string monoFont: "JetBrainsMono Nerd Font, JetBrains Mono, monospace"
    readonly property string iconFont: "JetBrainsMono Nerd Font, JetBrains Mono, monospace"

    // Nerd Font v3 Glyphs (Zero Emoji Policy)
    readonly property string iconRocket: "\uf135"
    readonly property string iconGamepad: "\uf11b"
    readonly property string iconCpu: String.fromCodePoint(0xF0EE0)
    readonly property string iconGpu: String.fromCodePoint(0xF08AE)
    readonly property string iconDisplay: String.fromCodePoint(0xF0379)
    readonly property string iconSpeed: String.fromCodePoint(0xF04C5)
    readonly property string iconCheck: "\uf00c"
    readonly property string iconRefresh: "\uf021"
    readonly property string iconWrench: "\uf0ad"

    property string lastLoadedRaw: ""

    function loadColors(raw) {
        if (!raw || raw.trim().length === 0 || raw === lastLoadedRaw) return
        lastLoadedRaw = raw

        var dict = {}
        var lines = String(raw).split("\n")
        for (var i = 0; i < lines.length; i++) {
            var line = lines[i].trim()
            if (!line || line.charAt(0) === '#') continue
            var match = line.match(/^([A-Za-z0-9_-]+)\s*=\s*["']?([^"'\r\n]+?)["']?\s*(?:#.*)?$/)
            if (match) {
                dict[match[1].toLowerCase()] = match[2].trim()
            }
        }

        var mode = dict["mode"] || "dark"
        root.isDarkTheme = (mode !== "light")

        var base = dict["background"] || dict["bg"] || (root.isDarkTheme ? "#111111" : "#f5f5f5")
        var fg = dict["foreground"] || dict["fg"] || (root.isDarkTheme ? "#e5e5e5" : "#1a1a1a")
        var acc = dict["accent"] || dict["color4"] || dict["color6"] || "#b8b8b8"

        root.bgBase = base
        root.textMain = dict["bright_foreground"] || fg
        root.accent = acc
        root.accentHover = Qt.lighter(acc, 1.15)
        root.accentSuccess = dict["bright_green"] || dict["green"] || "#a6da95"
        root.accentWarning = dict["yellow"] || dict["bright_yellow"] || "#eed49f"
        root.accentDanger = dict["red"] || dict["bright_red"] || "#ed8796"
    }

    property FileView colorsFile: FileView {
        id: colorsFile
        path: root.omarchyStateDir + "/theme/colors.toml"
        watchChanges: true
        printErrors: false
        onLoaded: root.loadColors(text())
        onFileChanged: reload()
    }
}
