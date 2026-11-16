import QtQuick
import omalink

// What dragging files over the window looks like: the page dims and one big
// target says where they'll go.
Item {
    id: overlay
    property bool active: false
    property bool accepting: true
    property string target: ""

    visible: opacity > 0
    opacity: active ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.quick } }

    Rectangle { anchors.fill: parent; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0.82) }

    Rectangle {
        id: well
        anchors { fill: parent; margins: Theme.s5 }
        radius: Theme.card
        color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, overlay.accepting ? 0.08 : 0.0)
        border.width: 2
        border.color: overlay.accepting ? Theme.accentLine : Theme.line
        scale: overlay.active ? 1 : 0.98
        Behavior on scale { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } }

        // A solid panel, so the words read over whatever page is behind.
        Rectangle {
            anchors.centerIn: parent
            width: message.width + Theme.s6 * 2; height: message.height + Theme.s6 * 2
            radius: Theme.card
            color: Theme.overlay
            border.width: 1; border.color: Theme.line
        }
        Column {
            id: message
            anchors.centerIn: parent
            spacing: Theme.s3
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 84; height: 84; radius: Theme.radius > 0 ? 42 : Theme.card
                color: overlay.accepting ? Theme.accentSoft : Theme.hover
                Icon { anchors.centerIn: parent; name: "upload"; size: 36; color: overlay.accepting ? Theme.accent : Theme.secondary }
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: overlay.accepting ? qsTr("Drop to send to %1").arg(overlay.target || qsTr("your phone")) : qsTr("Connect your phone to send files")
                color: Theme.foreground
                font.family: Theme.font; font.pixelSize: Theme.title; font.weight: Font.DemiBold
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: overlay.accepting
                text: qsTr("They land in Downloads/Omalink on the phone")
                color: Theme.secondary
                font.family: Theme.font; font.pixelSize: Theme.label
            }
        }
    }
}
