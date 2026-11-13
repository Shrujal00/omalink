import QtQuick
import QtQuick.Controls
import omalink

Item {
    id: page
    signal setup()

    Gate { id: gate; anchors.fill: parent; icon: "clipboard"; what: qsTr("Clipboard"); onSetup: page.setup() }

    Flickable {
        anchors.fill: parent
        visible: !gate.blocked
        contentHeight: column.height + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
            id: column
            width: Math.min(parent.width - 40, 680)
            anchors.horizontalCenter: parent.horizontalCenter
            y: 18
            spacing: 14

            Rectangle {
                width: parent.width; height: 68; radius: Theme.small; color: Theme.surface
                Column {
                    anchors { left: parent.left; leftMargin: 16; right: sync.left; rightMargin: 16; verticalCenter: parent.verticalCenter }
                    spacing: 3
                    Text { text: qsTr("Send what I copy to my phone"); color: Theme.foreground; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium }
                    Text { width: parent.width; elide: Text.ElideRight; text: qsTr("Text only. Passwords from password managers are never sent."); color: Theme.secondary; font.family: Theme.font; font.pixelSize: 12 }
                }
                Toggle { id: sync; anchors { right: parent.right; rightMargin: 16; verticalCenter: parent.verticalCenter }
                         on: Store.settings.clipboard; onToggled: on => Store.send("set", { key: "clipboard", value: on }) }
            }

            Row {
                width: parent.width
                spacing: 8
                Field { id: text; width: parent.width - 46; icon: "clipboard"; placeholder: qsTr("Type or paste something to send to your phone"); onAccepted: push.clicked() }
                Btn {
                    id: push
                    icon: "send"; kind: "primary"; implicitWidth: 38; implicitHeight: 36; hint: qsTr("Send to phone")
                    enabled: text.text !== ""
                    onClicked: { Store.send("send_clipboard", { text: text.text }); text.text = "" }
                }
            }

            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                text: qsTr("Android only lets an app read the clipboard while it is on screen. To send from the phone, select text and choose Share → Omalink, or tap “Send clipboard” in the Omalink notification.")
                color: Theme.tertiary
                font.family: Theme.font; font.pixelSize: 12
                lineHeight: 1.25
            }

            Text {
                topPadding: 8
                visible: Store.clips.length > 0
                text: qsTr("Recent")
                color: Theme.secondary
                font.family: Theme.font; font.pixelSize: 12; font.weight: Font.DemiBold
            }

            Repeater {
                model: Store.clips
                Rectangle {
                    id: clip
                    required property var modelData
                    property bool copied: false
                    width: column.width
                    height: Math.max(54, words.height + 34)
                    radius: Theme.small
                    color: over.hovered ? Theme.raised : Theme.surface
                    Behavior on color { ColorAnimation { duration: Theme.quick } }
                    HoverHandler { id: over }

                    Text {
                        id: words
                        anchors { left: parent.left; leftMargin: 14; right: again.left; rightMargin: 10; top: parent.top; topMargin: 10 }
                        text: clip.modelData.text
                        color: Theme.foreground
                        font.family: Theme.font; font.pixelSize: 13
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                    }
                    Text {
                        anchors { left: parent.left; leftMargin: 14; top: words.bottom; topMargin: 4 }
                        text: (clip.modelData.from === "phone" ? qsTr("From phone") : qsTr("From this computer")) + "  ·  " + Store.when(clip.modelData.time)
                        color: Theme.tertiary
                        font.family: Theme.font; font.pixelSize: 12
                    }
                    Btn {
                        id: again
                        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        icon: clip.copied ? "check" : "copy"; kind: "ghost"; implicitWidth: 32; implicitHeight: 32
                        tint: clip.copied ? Theme.green : Theme.secondary; hint: qsTr("Copy")
                        onClicked: { Store.send("copy", { text: clip.modelData.text }); clip.copied = true; reset.restart() }
                    }
                    Timer { id: reset; interval: 1400; onTriggered: clip.copied = false }
                }
            }
        }
    }
}
