import QtQuick
import omalink

// One line in a Section: a label, an explanation, and something on the right.
Item {
    id: row
    property string title: ""
    property string detail: ""
    property string icon: ""
    property color iconColor: Theme.secondary
    default property alias trailing: slot.data
    width: parent ? parent.width : 0
    height: Math.max(54, labels.height + 22)

    Icon { id: glyph; visible: row.icon !== ""; name: row.icon; size: 17; color: row.iconColor
           anchors { left: parent.left; leftMargin: 16; verticalCenter: parent.verticalCenter } }
    Column {
        id: labels
        anchors { left: glyph.visible ? glyph.right : parent.left; leftMargin: glyph.visible ? 13 : 16; right: slot.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
        spacing: 2
        Text { width: parent.width; elide: Text.ElideRight; text: row.title; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium }
        Text { visible: text !== ""; width: parent.width; wrapMode: Text.WordWrap; text: row.detail; color: Theme.secondary; font.family: Theme.font; font.pixelSize: 12 }
    }
    Item { id: slot; width: childrenRect.width; height: childrenRect.height; anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter } }
    Rectangle { height: 1; color: Theme.line; visible: row.y > 0; anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16 } }
}
