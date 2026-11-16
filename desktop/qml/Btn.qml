import QtQuick
import omalink

// The one button. `kind` picks its weight: "primary" for the main action on a
// surface, "soft" for the rest, "ghost" for actions inside a row, "danger".
Item {
    id: button
    property string text: ""
    property string icon: ""
    property string kind: "soft"
    property bool round: false
    property bool busy: false
    property real iconSize: 16
    property color tint: kind === "primary" ? Theme.onAccent : kind === "danger" ? Theme.red : Theme.foreground
    property string hint: ""
    signal clicked()

    implicitHeight: 34
    implicitWidth: round ? implicitHeight : row.implicitWidth + (text ? 28 : 18)
    opacity: enabled ? 1 : 0.4
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text || hint
    Keys.onReturnPressed: clicked()
    Keys.onSpacePressed: clicked()

    Rectangle {
        anchors.fill: parent
        radius: button.round ? height / 2 : Theme.small
        color: {
            const base = button.kind === "primary" ? Theme.accent
                       : button.kind === "soft" ? Theme.hover : "transparent"
            if (button.kind === "primary")
                return area.pressed ? Qt.darker(base, 1.15) : area.containsMouse ? Qt.lighter(base, 1.08) : base
            return area.pressed ? Theme.pressed : area.containsMouse ? (button.kind === "soft" ? Theme.pressed : Theme.hover) : base
        }
        border.width: button.activeFocus ? 1.5 : 0
        border.color: Theme.accent
        scale: area.pressed ? 0.97 : 1
        Behavior on color { ColorAnimation { duration: Theme.quick } }
        Behavior on scale { NumberAnimation { duration: 90 } }
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 7
        Icon {
            visible: button.icon !== ""
            name: button.icon
            size: button.iconSize
            color: button.tint
            anchors.verticalCenter: parent.verticalCenter
        }
        Text {
            visible: button.text !== ""
            text: button.text
            color: button.tint
            font.family: Theme.font
            font.pixelSize: 13
            font.weight: Font.Medium
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: button.clicked()
    }

    // An icon with no words says what it does on hover.
    Tip { text: button.hint; visible: area.containsMouse && button.hint !== "" && button.text === "" }
}
