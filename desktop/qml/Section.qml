import QtQuick
import omalink

// A titled group of rows on the Phone page.
Column {
    id: section
    property string title: ""
    property string detail: ""
    default property alias rows: body.data
    property alias trailing: slot.data
    spacing: 8

    Item {
        width: parent.width
        height: Math.max(heading.height, slot.childrenRect.height)
        Column {
            id: heading
            anchors { left: parent.left; right: slot.left; rightMargin: 12 }
            spacing: 2
            Text { text: section.title; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.DemiBold }
            Text { visible: text !== ""; width: parent.width; wrapMode: Text.WordWrap; text: section.detail; color: Theme.secondary; font.family: Theme.font; font.pixelSize: 12 }
        }
        Item { id: slot; width: childrenRect.width; height: childrenRect.height; anchors { right: parent.right; verticalCenter: parent.verticalCenter } }
    }
    Rectangle {
        width: parent.width
        height: body.height
        radius: Theme.small
        color: Theme.surface
        clip: true
        Column { id: body; width: parent.width }
    }
}
