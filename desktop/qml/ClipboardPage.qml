import QtQuick
import QtQuick.Controls
import omalink

// What's been copied on either side, as a wall of cards. Click one to copy
// it here; send it to the phone from its corner. The header's lightning bolt
// sends everything you copy automatically.
Item {
    id: page
    signal setup()

    Gate { id: gate; anchors.fill: parent; icon: "clipboard"; what: qsTr("Clipboard"); onSetup: page.setup() }

    Flickable {
        anchors.fill: parent
        visible: !gate.blocked
        contentHeight: column.height + Theme.s6 * 2
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
            id: column
            readonly property real margin: Theme.margin(page.width)
            width: Math.min(page.width - margin * 2, 1640)
            anchors.horizontalCenter: parent.horizontalCenter
            y: Theme.s5
            spacing: Theme.s5

            PageHeader {
                width: parent.width
                title: qsTr("Clipboard")

                Btn {
                    anchors.verticalCenter: parent.verticalCenter
                    icon: "info"; iconSize: 16; kind: "ghost"; tint: Theme.tertiary; implicitWidth: 36
                    hint: qsTr("From the phone: Share → Omalink, or “Send clipboard” in its notification")
                }
                // Automatic sending, on or off.
                Btn {
                    anchors.verticalCenter: parent.verticalCenter
                    icon: "bolt"; iconSize: 17; implicitWidth: 42; implicitHeight: 38
                    kind: Store.settings.clipboard ? "primary" : "soft"
                    tint: Store.settings.clipboard ? Theme.onAccent : Theme.secondary
                    hint: Store.settings.clipboard ? qsTr("Sending what you copy to the phone · click to stop")
                                                   : qsTr("Send what you copy to the phone automatically")
                    onClicked: Store.send("set", { key: "clipboard", value: !Store.settings.clipboard })
                }
            }

            // Type or paste, then send.
            Rectangle {
                width: parent.width; height: 52
                radius: Theme.card
                color: Theme.surface
                border.width: 1; border.color: input.activeFocus ? Theme.accentLine : Theme.line
                Icon { id: lead; name: "clipboard"; size: 18; color: Theme.tertiary
                       anchors { left: parent.left; leftMargin: Theme.s4; verticalCenter: parent.verticalCenter } }
                TextInput {
                    id: input
                    anchors { left: lead.right; leftMargin: Theme.s3; right: push.left; rightMargin: Theme.s3; verticalCenter: parent.verticalCenter }
                    color: Theme.foreground
                    font.family: Theme.font; font.pixelSize: Theme.body
                    clip: true
                    selectByMouse: true
                    onAccepted: push.clicked()
                    Text { visible: !input.text; text: qsTr("Send text to the phone"); color: Theme.tertiary; font: input.font; anchors.verticalCenter: parent.verticalCenter }
                }
                Btn {
                    id: push
                    anchors { right: parent.right; rightMargin: 7; verticalCenter: parent.verticalCenter }
                    icon: "send"; kind: "primary"; implicitWidth: 40; implicitHeight: 38; hint: qsTr("Send to phone")
                    enabled: input.text !== ""
                    onClicked: { Store.send("send_clipboard", { text: input.text }); input.text = "" }
                }
            }

            Empty {
                visible: Store.clips.length === 0
                anchors.horizontalCenter: parent.horizontalCenter
                topPadding: Theme.s6
                icon: "clipboard"
                title: qsTr("Nothing copied yet")
            }

            Flow {
                id: wall
                width: parent.width
                spacing: Theme.s4
                readonly property int columns: Math.max(1, Math.floor((width + spacing) / (260 + spacing)))
                readonly property real cell: (width - spacing * (columns - 1)) / columns

                Repeater {
                    model: Store.clips
                    Card {
                        id: clip
                        required property var modelData
                        property bool copied: false
                        width: wall.cell
                        height: 132
                        onClicked: { Store.send("copy", { text: clip.modelData.text }); clip.copied = true; reset.restart() }

                        Text {
                            anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.s4 }
                            text: clip.modelData.text
                            color: Theme.foreground
                            font.family: Theme.font; font.pixelSize: Theme.label
                            wrapMode: Text.Wrap
                            maximumLineCount: 4
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            lineHeight: 1.15
                        }
                        // Where it came from, and when.
                        Row {
                            anchors { left: parent.left; bottom: parent.bottom; margins: Theme.s4; bottomMargin: 12 }
                            spacing: 6
                            Icon { name: clip.modelData.from === "phone" ? "device" : "laptop"; size: 14; color: Theme.tertiary; anchors.verticalCenter: parent.verticalCenter }
                            Text { text: Store.when(clip.modelData.time); color: Theme.tertiary; font.family: Theme.font; font.pixelSize: Theme.caption; anchors.verticalCenter: parent.verticalCenter }
                        }
                        Row {
                            anchors { right: parent.right; bottom: parent.bottom; margins: 8 }
                            spacing: 2
                            opacity: clip.hovered || clip.copied ? 1 : 0
                            Behavior on opacity { NumberAnimation { duration: Theme.quick } }
                            Btn { icon: "send"; iconSize: 14; kind: "ghost"; tint: Theme.secondary; implicitWidth: 30; implicitHeight: 30; hint: qsTr("Send to phone")
                                  onClicked: Store.send("send_clipboard", { text: clip.modelData.text }) }
                            Btn { icon: clip.copied ? "check" : "copy"; iconSize: 14; kind: "ghost"; implicitWidth: 30; implicitHeight: 30
                                  tint: clip.copied ? Theme.green : Theme.secondary; hint: qsTr("Copy here")
                                  onClicked: { Store.send("copy", { text: clip.modelData.text }); clip.copied = true; reset.restart() } }
                        }
                        Timer { id: reset; interval: 1400; onTriggered: clip.copied = false }
                    }
                }
            }
        }
    }
}
