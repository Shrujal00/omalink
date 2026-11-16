import QtQuick
import QtQuick.Controls
import omalink

// What an icon means, shown after a short hover. `side` puts it to the
// right of its parent (for the rail) instead of above it.
ToolTip {
    id: tip
    property string side: "top"
    delay: 350
    timeout: 5000
    padding: 0
    x: side === "right" ? parent.width + 10 : (parent.width - implicitWidth) / 2
    y: side === "right" ? (parent.height - implicitHeight) / 2 : -implicitHeight - 8

    contentItem: Text {
        text: tip.text
        color: Theme.foreground
        font.family: Theme.font; font.pixelSize: Theme.label; font.weight: Font.Medium
        leftPadding: 10; rightPadding: 10; topPadding: 6; bottomPadding: 6
    }
    background: Rectangle {
        radius: Theme.small
        color: Theme.overlay
        border.width: 1; border.color: Theme.line
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.quick } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 90 } }
}
