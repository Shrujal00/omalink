import QtQuick
import omalink

// A switch.
Item {
    id: toggle
    property bool on: false
    signal toggled(bool on)
    implicitWidth: 38; implicitHeight: 22
    activeFocusOnTab: true
    Accessible.role: Accessible.CheckBox
    Accessible.checked: on
    Keys.onSpacePressed: toggled(!on)

    Rectangle {
        anchors.fill: parent
        radius: Math.min(height / 2, Theme.radius > 0 ? height / 2 : 3)
        color: toggle.on ? Theme.accent : Theme.pressed
        border.width: toggle.activeFocus ? 1.5 : 0
        border.color: Theme.foreground
        Behavior on color { ColorAnimation { duration: Theme.quick } }
        Rectangle {
            width: 16; height: 16; radius: parent.radius > 3 ? 8 : 2
            y: 3
            x: toggle.on ? parent.width - width - 3 : 3
            color: toggle.on ? Theme.onAccent : Theme.secondary
            Behavior on x { NumberAnimation { duration: Theme.quick; easing.type: Easing.OutCubic } }
        }
    }
    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: toggle.toggled(!toggle.on) }
}
