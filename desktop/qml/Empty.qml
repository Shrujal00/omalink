import QtQuick
import omalink

// What a list shows when it has nothing, and why.
Column {
    id: empty
    property string icon: ""
    property string title: ""
    property string detail: ""
    property string action: ""
    signal triggered()
    spacing: 10
    width: Math.min(340, parent ? parent.width - 48 : 340)

    Rectangle {
        width: 52; height: 52; radius: 26
        color: Theme.hover
        anchors.horizontalCenter: parent.horizontalCenter
        Icon { anchors.centerIn: parent; name: empty.icon; size: 22; color: Theme.secondary }
    }
    Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: empty.title
        color: Theme.foreground
        font.family: Theme.font; font.pixelSize: 15; font.weight: Font.DemiBold
        wrapMode: Text.WordWrap
    }
    Text {
        width: parent.width
        visible: text !== ""
        horizontalAlignment: Text.AlignHCenter
        text: empty.detail
        color: Theme.secondary
        font.family: Theme.font; font.pixelSize: 13
        wrapMode: Text.WordWrap
        lineHeight: 1.25
    }
    Btn {
        visible: empty.action !== ""
        text: empty.action
        kind: "primary"
        anchors.horizontalCenter: parent.horizontalCenter
        onClicked: empty.triggered()
    }
}
