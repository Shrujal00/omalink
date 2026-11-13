import QtQuick
import omalink

// A single-line text input with an optional leading icon.
Rectangle {
    id: field
    property alias text: input.text
    property string placeholder: ""
    property string icon: ""
    property alias input: input
    signal accepted()

    implicitHeight: 36
    radius: Theme.small
    color: Theme.hover
    border.width: 1
    border.color: input.activeFocus ? Theme.accent : "transparent"
    Behavior on border.color { ColorAnimation { duration: Theme.quick } }

    Icon {
        id: lead
        visible: field.icon !== ""
        name: field.icon
        size: 15
        color: Theme.tertiary
        anchors { left: parent.left; leftMargin: 11; verticalCenter: parent.verticalCenter }
    }
    TextInput {
        id: input
        anchors { left: lead.visible ? lead.right : parent.left; leftMargin: lead.visible ? 8 : 12; right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
        color: Theme.foreground
        selectionColor: Theme.accentSoft
        selectedTextColor: Theme.foreground
        font.family: Theme.font
        font.pixelSize: 13
        clip: true
        selectByMouse: true
        onAccepted: field.accepted()
        Text {
            visible: !input.text && !input.preeditText
            text: field.placeholder
            color: Theme.tertiary
            font: input.font
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
