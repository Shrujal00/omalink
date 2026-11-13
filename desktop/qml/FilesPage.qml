import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import omalink

// Files to and from the phone. They travel over the network, not Bluetooth,
// so both have to be on the same one.
Item {
    id: page
    signal setup()

    function send(urls) {
        const paths = []
        for (const url of urls) {
            const text = String(url)
            if (text.startsWith("file://")) paths.push(decodeURIComponent(text.slice(7)))
        }
        if (paths.length) Store.send("send_files", { paths: paths })
    }
    function choose() { picker.open() }

    function size(bytes) {
        if (bytes < 0) return ""
        if (bytes < 1024) return qsTr("%1 B").arg(bytes)
        if (bytes < 1024 * 1024) return qsTr("%1 KB").arg(Math.round(bytes / 1024))
        if (bytes < 1024 * 1024 * 1024) return qsTr("%1 MB").arg((bytes / 1048576).toFixed(1))
        return qsTr("%1 GB").arg((bytes / 1073741824).toFixed(2))
    }

    FileDialog {
        id: picker
        title: qsTr("Send to phone")
        fileMode: FileDialog.OpenFiles
        onAccepted: page.send(selectedFiles)
    }

    Gate { id: gate; anchors.fill: parent; icon: "file"; what: qsTr("Files"); onSetup: page.setup() }

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
                width: parent.width; height: 132; radius: Theme.small
                color: Theme.surface
                border.width: 1; border.color: Theme.line
                Column {
                    anchors.centerIn: parent
                    spacing: 10
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("Drop files anywhere in this window to send them to your phone")
                        color: Theme.foreground
                        font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium
                    }
                    Btn { anchors.horizontalCenter: parent.horizontalCenter; text: qsTr("Choose files"); icon: "plus"; kind: "primary"; onClicked: page.choose() }
                }
            }

            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                text: qsTr("Files go over Wi-Fi, encrypted, straight between the phone and this computer, so both need to be on the same network. They land in Downloads here and in Downloads/Omalink on the phone. To send from the phone, share a file and choose Omalink.")
                color: Theme.tertiary
                font.family: Theme.font; font.pixelSize: 12
                lineHeight: 1.25
            }

            Item {
                width: parent.width; height: 30
                visible: Store.transfers.length > 0
                Text {
                    anchors { left: parent.left; bottom: parent.bottom; bottomMargin: 2 }
                    text: qsTr("Recent")
                    color: Theme.secondary
                    font.family: Theme.font; font.pixelSize: 12; font.weight: Font.DemiBold
                }
                Btn { anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                      text: qsTr("Clear"); kind: "ghost"; implicitHeight: 28; tint: Theme.secondary
                      onClicked: Store.send("clear_transfers") }
            }

            Repeater {
                model: Store.transfers
                Rectangle {
                    id: item
                    required property var modelData
                    readonly property bool moving: modelData.state === "active" || modelData.state === "waiting"
                    readonly property bool here: modelData.incoming && modelData.state === "done"
                    width: column.width; height: 62; radius: Theme.small
                    color: over.hovered ? Theme.raised : Theme.surface
                    Behavior on color { ColorAnimation { duration: Theme.quick } }
                    HoverHandler { id: over }

                    Rectangle {
                        id: badge
                        width: 34; height: 34; radius: Theme.radius > 0 ? 17 : 0
                        anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter }
                        color: item.modelData.state === "failed" ? Qt.rgba(Theme.red.r, Theme.red.g, Theme.red.b, 0.16) : Theme.accentSoft
                        Icon { anchors.centerIn: parent; size: 16
                               name: item.modelData.state === "failed" ? "x" : item.modelData.incoming ? "in" : "out"
                               color: item.modelData.state === "failed" ? Theme.red : Theme.accent }
                    }
                    Column {
                        anchors { left: badge.right; leftMargin: 12; right: buttons.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        spacing: 4
                        Text {
                            width: parent.width; elide: Text.ElideMiddle
                            text: item.modelData.name
                            color: Theme.foreground
                            font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium
                            textFormat: Text.PlainText
                        }
                        Text {
                            width: parent.width; elide: Text.ElideRight
                            visible: !item.moving
                            text: item.modelData.state === "failed"
                                  ? item.modelData.error
                                  : (item.modelData.incoming ? qsTr("From phone") : qsTr("Sent to phone")) + "  ·  " + page.size(item.modelData.size) + "  ·  " + Store.when(item.modelData.time)
                            color: item.modelData.state === "failed" ? Theme.red : Theme.tertiary
                            font.family: Theme.font; font.pixelSize: 12
                        }
                        // Progress, while there is some to show.
                        Rectangle {
                            visible: item.moving
                            width: parent.width; height: 4; radius: 2
                            color: Theme.pressed
                            Rectangle {
                                height: parent.height; radius: 2; color: Theme.accent
                                width: item.modelData.size > 0 ? parent.width * Math.min(1, item.modelData.done / item.modelData.size) : 0
                                Behavior on width { NumberAnimation { duration: Theme.quick } }
                            }
                        }
                        Text {
                            visible: item.moving
                            text: item.modelData.state === "waiting" ? qsTr("Waiting for the phone…")
                                : page.size(item.modelData.done) + (item.modelData.size > 0 ? " / " + page.size(item.modelData.size) : "")
                            color: Theme.tertiary
                            font.family: Theme.font; font.pixelSize: 11
                        }
                    }
                    Row {
                        id: buttons
                        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        spacing: 2
                        Btn { visible: item.here; text: qsTr("Open"); kind: "ghost"; implicitHeight: 30; tint: Theme.accent
                              onClicked: Store.send("open_path", { path: item.modelData.path }) }
                        Btn { visible: item.here; icon: "folder"; kind: "ghost"; implicitWidth: 32; implicitHeight: 30; tint: Theme.secondary; hint: qsTr("Show in folder")
                              onClicked: Store.send("show_path", { path: item.modelData.path }) }
                    }
                }
            }
        }
    }
}
