import QtQuick
import QtQuick.Shapes
import omalink

// Line icons drawn as paths on a 24-unit grid, so they take any color and
// stay sharp at any size without shipping image files.
Item {
    id: icon
    property string name: ""
    property color color: Theme.foreground
    property real size: 18
    property real weight: 1.6
    implicitWidth: size
    implicitHeight: size

    readonly property var paths: ({
        "message": "M4 5h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H9.5L5 20.5V17H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z",
        "chat": "M4 4.5h11a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H9L5.5 15v-2.5H4a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1zM18.5 9.5H20a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-1.5V20L15 17.5h-4",
        "phone": "M6.5 3h3l1.5 4-2 1.5a12 12 0 0 0 6.5 6.5l1.5-2 4 1.5v3a2 2 0 0 1-2 2A16 16 0 0 1 4.5 5a2 2 0 0 1 2-2z",
        "bell": "M6 16v-5a6 6 0 0 1 12 0v5l1.5 2h-15zM10 20.5a2 2 0 0 0 4 0",
        "clipboard": "M9 3.5h6v3H9zM9 5H6.5a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1H15",
        "device": "M8 2.5h8A1.5 1.5 0 0 1 17.5 4v16a1.5 1.5 0 0 1-1.5 1.5H8A1.5 1.5 0 0 1 6.5 20V4A1.5 1.5 0 0 1 8 2.5zM11 18.5h2",
        "search": "M10.5 4a6.5 6.5 0 1 1 0 13a6.5 6.5 0 0 1 0-13zM15.5 15.5L20 20",
        "send": "M4 12L20 4l-5 16-3-7zM12 13l8-9",
        "x": "M6 6l12 12M18 6L6 18",
        "check": "M5 12.5l4.5 4.5L19 7.5",
        "plus": "M12 5v14M5 12h14",
        "backspace": "M9 5h11a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H9l-6-7zM12 9.5l5 5M17 9.5l-5 5",
        "mic": "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 17v4",
        "mic-off": "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 17v4M4 4l16 16",
        "in": "M17 7L7 17M7 9v8h8",
        "out": "M7 17L17 7M9 7h8v8",
        "refresh": "M20 12a8 8 0 1 1-2.5-5.8M20 4v5h-5",
        "trash": "M5 7h14M10 7V4.5h4V7M7 7l1 13h8l1-13",
        "bluetooth": "M7 7.5l10 9-5 4.5V3l5 4.5-10 9",
        "copy": "M9 9h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V10a1 1 0 0 1 1-1zM5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1",
        "keypad": "M6 6h.01M12 6h.01M18 6h.01M6 12h.01M12 12h.01M18 12h.01M6 18h.01M12 18h.01M18 18h.01",
        "shield": "M12 3l7 3v5c0 5-3 8.5-7 10-4-1.5-7-5-7-10V6z",
        "ring": "M12 8a4 4 0 1 1 0 8a4 4 0 0 1 0-8zM4.5 7a9 9 0 0 0 0 10M19.5 7a9 9 0 0 1 0 10",
        "chevron": "M9 6l6 6-6 6",
        "reply": "M10 7l-5 5 5 5M5 12h9a5 5 0 0 1 5 5v1",
        "bolt": "M13 3L5 13.5h6L10 21l8-10.5h-6z",
        "file": "M7 3h7l4 4v13a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM14 3v4h4",
        "folder": "M3 7a1 1 0 0 1 1-1h5l2 2h8a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"
    })

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: icon.color
            strokeWidth: icon.weight * (icon.name === "keypad" ? 1.5 : 1)
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            scale: Qt.size(icon.size / 24, icon.size / 24)
            PathSvg { path: icon.paths[icon.name] || "" }
        }
    }
}
