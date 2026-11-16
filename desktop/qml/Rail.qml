import QtQuick
import omalink

// The way around: a slim column of icons. The phone sits on top, wearing its
// wallpaper inside its battery ring; a soft pill glides to the page you're
// on; names appear on hover.
Rectangle {
    id: rail
    property var pages: []
    property int current: 0
    signal pick(int index)

    width: 78
    color: Theme.sidebar

    readonly property var battery: Store.status && Store.connected ? Store.status.battery : -1

    // The phone: home.
    Item {
        id: me
        width: parent.width; height: 86
        HoverHandler { id: meHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: rail.pick(0) }

        ProgressRing {
            anchors.centerIn: avatar
            width: 58; height: 58; thickness: 3
            value: rail.battery >= 0 ? rail.battery / 100 : 0
            color: Store.status && Store.status.charging ? Theme.green : rail.battery >= 0 && rail.battery <= 15 ? Theme.red : Theme.accent
            visible: rail.battery >= 0
        }
        // Home is where the phone is: a soft pill behind it when Home is open.
        Rectangle {
            anchors.centerIn: avatar
            width: 70; height: 70; radius: Theme.radius > 0 ? 22 : Theme.small
            color: Theme.accentSoft
            opacity: rail.current === 0 ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.smooth } }
        }
        Rectangle {
            id: avatar
            anchors.centerIn: parent
            width: 46; height: 46; radius: 23
            color: Store.connected ? Theme.accentSoft : Theme.hover
            scale: meHover.hovered ? 1.08 : 1
            Behavior on scale { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutBack } }
            Icon { visible: !face.ready; anchors.centerIn: parent; name: "device"; size: 20; color: Store.connected ? Theme.accent : Theme.tertiary }
            Picture { id: face; anchors.fill: parent; radius: 23; source: Store.wallpaper; opacity: Store.connected ? 1 : 0.45 }
        }
        Rectangle {
            width: 12; height: 12; radius: 6
            anchors { right: avatar.right; bottom: avatar.bottom; margins: -2 }
            color: Store.connected ? Theme.green : Store.link.state === "connecting" ? Theme.yellow : Theme.tertiary
            border.width: 2.5; border.color: Theme.sidebar
        }
        Tip {
            side: "right"
            visible: meHover.hovered
            text: (Store.link.name || qsTr("Your phone")) + (rail.battery >= 0 ? "  ·  " + rail.battery + "%" : "")
        }
    }

    Rectangle { anchors { top: me.bottom; horizontalCenter: parent.horizontalCenter } width: 28; height: 1; color: Theme.line }

    // The pill under the current page.
    Rectangle {
        id: pill
        // Worked out from positions in the lists rather than read from the
        // items, which may not have been laid out yet.
        readonly property bool atBottom: rail.current >= rail.firstBottom
        visible: rail.current > 0
        x: (rail.width - width) / 2
        y: atBottom ? bottomColumn.y + (rail.current - rail.firstBottom) * 50
                    : column.y + (rail.current - 1) * 50
        width: 52; height: 44
        radius: Theme.radius > 0 ? 14 : Theme.small
        color: Theme.accentSoft
        Behavior on y { SpringAnimation { spring: 4; damping: 0.32; epsilon: 0.2 } }
        Rectangle { width: 3; height: 18; radius: 1.5; color: Theme.accent; anchors { left: parent.left; leftMargin: -13; verticalCenter: parent.verticalCenter } }
    }

    readonly property int firstBottom: {
        for (let i = 1; i < pages.length; i++) if (pages[i].bottom) return i
        return pages.length
    }

    Column {
        id: column
        anchors { top: me.bottom; topMargin: 14; horizontalCenter: parent.horizontalCenter }
        spacing: 6
        Repeater {
            id: icons
            model: rail.pages.slice(1, rail.firstBottom)
            RailIcon { index_: index + 1; page: modelData }
        }
    }

    Column {
        id: bottomColumn
        anchors { bottom: parent.bottom; bottomMargin: 14; horizontalCenter: parent.horizontalCenter }
        spacing: 6
        Repeater {
            id: bottomIcons
            model: rail.pages.slice(rail.firstBottom)
            RailIcon { index_: index + rail.firstBottom; page: modelData }
        }
    }

    component RailIcon: Item {
        id: item
        required property int index
        required property var modelData
        property int index_: 0
        property var page: ({})
        readonly property bool selected: rail.current === index_
        width: 52; height: 44

        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: rail.pick(item.index_) }

        Rectangle {
            anchors.fill: parent
            radius: Theme.radius > 0 ? 14 : Theme.small
            color: hover.hovered && !item.selected ? Theme.hover : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.quick } }
        }
        Icon {
            anchors.centerIn: parent
            name: item.page.icon
            size: 21
            color: item.selected ? Theme.accent : hover.hovered ? Theme.foreground : Theme.secondary
            scale: hover.hovered ? 1.12 : 1
            Behavior on scale { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutBack } }
            Behavior on color { ColorAnimation { duration: Theme.quick } }
        }
        // A count, or just a dot for "something's going on".
        Rectangle {
            visible: item.page.badge > 0
            anchors { right: parent.right; top: parent.top; rightMargin: 6; topMargin: 4 }
            height: 16; width: Math.max(16, count.implicitWidth + 8); radius: 8
            color: item.page.hot ? Theme.accent : Theme.raised
            border.width: 2; border.color: Theme.sidebar
            Text {
                id: count; anchors.centerIn: parent
                text: item.page.badge > 99 ? "99+" : item.page.badge
                color: item.page.hot ? Theme.onAccent : Theme.foreground
                font.family: Theme.font; font.pixelSize: 9; font.weight: Font.Bold
            }
        }
        Tip { side: "right"; visible: hover.hovered; text: item.page.label + (item.page.key ? "   " + item.page.key : "") }
    }
}
