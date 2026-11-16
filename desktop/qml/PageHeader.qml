import QtQuick
import omalink

// A page's title and what it's about, with its main actions on the right.
Item {
    id: header
    property string title: ""
    property string subtitle: ""
    default property alias actions: slot.data
    // In place of the title: tabs, say.
    property alias lead: leadSlot.data
    implicitHeight: Math.max(labels.implicitHeight, slot.childrenRect.height, leadSlot.childrenRect.height)

    Item {
        id: leadSlot
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        width: childrenRect.width; height: childrenRect.height
    }

    Column {
        id: labels
        visible: header.title !== ""
        anchors { left: parent.left; right: slot.left; rightMargin: Theme.s4; verticalCenter: parent.verticalCenter }
        spacing: 3
        Text {
            width: parent.width; elide: Text.ElideRight
            text: header.title
            color: Theme.foreground
            font.family: Theme.font; font.pixelSize: Theme.display; font.weight: Font.DemiBold
        }
        Text {
            visible: text !== ""
            width: parent.width; wrapMode: Text.WordWrap
            text: header.subtitle
            color: Theme.secondary
            font.family: Theme.font; font.pixelSize: Theme.label
        }
    }
    Row {
        id: slot
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: Theme.s2
    }
}
