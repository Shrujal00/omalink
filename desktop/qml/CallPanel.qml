import QtQuick
import omalink

// Slides down over the window while a call is ringing or in progress.
Rectangle {
    id: panel
    readonly property var call: Store.call
    readonly property bool live: call.phase !== "idle"
    property bool keypad: false
    property int elapsed: 0

    width: 340
    height: body.height + 32
    radius: Math.max(Theme.small, Math.min(16, Theme.radius))
    color: Theme.raised
    border.width: 1
    border.color: Theme.line
    opacity: live ? 1 : 0
    y: live ? 18 : -height - 20
    visible: opacity > 0
    onLiveChanged: if (!live) keypad = false
    Behavior on y { NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }
    Behavior on opacity { NumberAnimation { duration: Theme.smooth } }
    Behavior on height { NumberAnimation { duration: Theme.quick; easing.type: Easing.OutCubic } }

    Timer {
        interval: 1000; repeat: true; triggeredOnStart: true
        running: panel.call.phase === "active"
        onTriggered: panel.elapsed = Date.now() - panel.call.since
    }

    Column {
        id: body
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 16 }
        spacing: 14

        Row {
            width: parent.width
            spacing: 12
            Item {
                width: 44; height: 44
                // A ring pulses outward while the phone is ringing.
                Rectangle {
                    anchors.centerIn: parent
                    width: 44; height: 44; radius: 22
                    color: "transparent"
                    border.width: 2; border.color: Theme.green
                    visible: panel.call.phase === "incoming"
                    SequentialAnimation on scale { running: panel.call.phase === "incoming"; loops: Animation.Infinite
                        NumberAnimation { from: 1; to: 1.5; duration: 1100; easing.type: Easing.OutCubic } }
                    SequentialAnimation on opacity { running: panel.call.phase === "incoming"; loops: Animation.Infinite
                        NumberAnimation { from: 0.8; to: 0; duration: 1100; easing.type: Easing.OutCubic } }
                }
                Avatar { anchors.centerIn: parent; size: 44; name: panel.call.name || panel.call.number || "?" }
            }
            Column {
                width: parent.width - 56
                anchors.verticalCenter: parent.verticalCenter
                spacing: 3
                Text {
                    width: parent.width; elide: Text.ElideRight
                    text: panel.call.name || panel.call.number || qsTr("Unknown caller")
                    color: Theme.foreground
                    font.family: Theme.font; font.pixelSize: 15; font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width; elide: Text.ElideRight
                    text: panel.call.phase === "incoming" ? qsTr("Incoming call")
                        : panel.call.phase === "dialing" ? qsTr("Calling…")
                        : Store.timer(panel.elapsed) + (panel.call.audio ? "" : "  ·  " + qsTr("on the phone"))
                    color: panel.call.phase === "active" ? Theme.green : Theme.secondary
                    font.family: Theme.font; font.pixelSize: 13
                }
            }
        }

        Grid {
            visible: panel.keypad
            columns: 3; spacing: 6
            anchors.horizontalCenter: parent.horizontalCenter
            Repeater {
                model: ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"]
                Btn { required property string modelData; text: modelData; implicitWidth: 64; implicitHeight: 38
                      onClicked: Store.send("tone", { tone: modelData }) }
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 10
            Btn {
                visible: panel.call.phase === "active" && panel.call.audio
                icon: panel.call.muted ? "mic-off" : "mic"; round: true; implicitHeight: 42
                tint: panel.call.muted ? Theme.red : Theme.foreground; hint: qsTr("Mute")
                onClicked: Store.send("mute", { on: !panel.call.muted })
            }
            Btn {
                visible: panel.call.phase === "active" && panel.call.audio
                icon: "keypad"; round: true; implicitHeight: 42; hint: qsTr("Keypad")
                tint: panel.keypad ? Theme.accent : Theme.foreground
                onClicked: panel.keypad = !panel.keypad
            }
            Rectangle {
                visible: panel.call.phase === "incoming"
                width: 96; height: 42; radius: Theme.radius > 0 ? 21 : 0
                color: yes.pressed ? Qt.darker(Theme.green, 1.15) : Theme.green
                Row { anchors.centerIn: parent; spacing: 7
                    Icon { name: "phone"; size: 16; color: "#10140f"; weight: 1.8; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: qsTr("Answer"); color: "#10140f"; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.DemiBold } }
                MouseArea { id: yes; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: Store.send("answer") }
            }
            Rectangle {
                width: panel.call.phase === "incoming" ? 96 : 64; height: 42; radius: Theme.radius > 0 ? 21 : 0
                color: no.pressed ? Qt.darker(Theme.red, 1.15) : Theme.red
                Row { anchors.centerIn: parent; spacing: 7
                    Icon { name: "phone"; size: 16; color: "#1a0e0f"; weight: 1.8; rotation: 135; anchors.verticalCenter: parent.verticalCenter }
                    Text { visible: panel.call.phase === "incoming"; text: qsTr("Decline"); color: "#1a0e0f"; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.DemiBold } }
                MouseArea { id: no; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: Store.send("hangup") }
            }
        }
    }
}
