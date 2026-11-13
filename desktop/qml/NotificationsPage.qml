import QtQuick
import QtQuick.Controls
import omalink

Item {
    id: page
    signal setup()
    property string replying: ""

    Gate { id: gate; anchors.fill: parent; permission: "notifications"; icon: "bell"; what: qsTr("Notifications"); onSetup: page.setup() }

    Item {
        anchors.fill: parent
        visible: !gate.blocked

        Item {
            id: bar
            height: 58
            width: Math.min(parent.width - 40, 680)
            anchors.horizontalCenter: parent.horizontalCenter
            Text {
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                text: Store.notifications.length === 0 ? qsTr("Notifications")
                    : Store.notifications.length === 1 ? qsTr("1 notification") : qsTr("%1 notifications").arg(Store.notifications.length)
                color: Theme.foreground
                font.family: Theme.font; font.pixelSize: 15; font.weight: Font.DemiBold
            }
            Btn {
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                visible: Store.notifications.some(n => n.clearable)
                text: qsTr("Clear all"); kind: "ghost"; tint: Theme.secondary; implicitHeight: 30
                onClicked: Store.send("dismiss_all")
            }
        }

        ListView {
            id: list
            anchors { top: bar.bottom; bottom: parent.bottom; horizontalCenter: parent.horizontalCenter }
            width: bar.width
            clip: true
            spacing: 8
            bottomMargin: 20
            model: Store.notifications
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded; anchors.left: parent.right; anchors.leftMargin: 6 }
            delegate: Rectangle {
                id: card
                required property var modelData
                readonly property bool replying: page.replying === modelData.key
                width: list.width
                height: content.height + 26
                radius: Theme.small
                color: Theme.surface

                HoverHandler { id: over }
                Column {
                    id: content
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 14; topMargin: 13 }
                    spacing: 6

                    Row {
                        width: parent.width
                        spacing: 8
                        Rectangle {
                            width: 18; height: 18; radius: 4; color: "transparent"
                            anchors.verticalCenter: parent.verticalCenter
                            Image { id: glyph; anchors.fill: parent; source: card.modelData.icon; sourceSize: Qt.size(36, 36); visible: status === Image.Ready }
                            Icon { anchors.centerIn: parent; visible: !glyph.visible; name: "bell"; size: 14; color: Theme.tertiary }
                        }
                        Text {
                            text: card.modelData.app
                            color: Theme.secondary
                            font.family: Theme.font; font.pixelSize: 12; font.weight: Font.Medium
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Text {
                            text: "·  " + Store.when(card.modelData.time)
                            color: Theme.tertiary
                            font.family: Theme.font; font.pixelSize: 12
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    Text {
                        width: parent.width - 30
                        visible: text !== ""
                        text: card.modelData.title
                        color: Theme.foreground
                        font.family: Theme.font; font.pixelSize: 14; font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                    }
                    Text {
                        width: parent.width
                        visible: text !== ""
                        text: card.modelData.text
                        color: Theme.secondary
                        font.family: Theme.font; font.pixelSize: 13
                        wrapMode: Text.Wrap
                        maximumLineCount: 6
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        lineHeight: 1.2
                    }
                    Flow {
                        width: parent.width
                        spacing: 6
                        topPadding: 2
                        visible: !card.replying && (card.modelData.can_reply || card.modelData.actions.length > 0)
                        Btn {
                            visible: card.modelData.can_reply
                            text: qsTr("Reply"); icon: "reply"; iconSize: 14; implicitHeight: 30; tint: Theme.accent
                            onClicked: { page.replying = card.modelData.key; answer.input.forceActiveFocus() }
                        }
                        Repeater {
                            model: card.modelData.actions
                            Btn {
                                required property string modelData
                                required property int index
                                text: modelData; implicitHeight: 30
                                onClicked: Store.send("action", { key: card.modelData.key, index: index })
                            }
                        }
                    }
                    Row {
                        width: parent.width
                        spacing: 8
                        visible: card.replying
                        topPadding: 2
                        Field {
                            id: answer
                            width: parent.width - 46
                            placeholder: qsTr("Reply to %1").arg(card.modelData.title || card.modelData.app)
                            onAccepted: send.clicked()
                            Keys.onEscapePressed: page.replying = ""
                        }
                        Btn {
                            id: send
                            icon: "send"; kind: "primary"; implicitWidth: 38; implicitHeight: 36; hint: qsTr("Send")
                            enabled: answer.text.trim() !== ""
                            onClicked: {
                                Store.send("reply", { key: card.modelData.key, text: answer.text.trim() })
                                answer.text = ""
                                page.replying = ""
                            }
                        }
                    }
                }
                Btn {
                    anchors { right: parent.right; top: parent.top; margins: 8 }
                    visible: card.modelData.clearable
                    opacity: over.hovered ? 1 : 0
                    icon: "x"; iconSize: 14; kind: "ghost"; implicitWidth: 28; implicitHeight: 28; tint: Theme.secondary; hint: qsTr("Dismiss")
                    onClicked: Store.send("dismiss", { key: card.modelData.key })
                    Behavior on opacity { NumberAnimation { duration: Theme.quick } }
                }
            }
            add: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.smooth } }
            displaced: Transition { NumberAnimation { properties: "y"; duration: Theme.smooth; easing.type: Easing.OutCubic } }

            Empty {
                anchors.centerIn: parent
                visible: list.count === 0
                icon: "bell"
                title: qsTr("All caught up")
                detail: qsTr("New notifications from your phone show up here and on your desktop.")
            }
        }
    }
}
