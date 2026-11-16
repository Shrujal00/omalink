import QtQuick
import QtQuick.Controls
import QtMultimedia
import omalink

// The phone's screen, live, after an icon check on the phone. With control
// switched on there, the pointer is a finger: click to tap, drag to swipe,
// the wheel scrolls, right-click goes back, and typing types.
Item {
    id: page
    property bool current: false
    signal openPhone()
    readonly property var m: Store.mirror
    readonly property bool live: m.state === "live"
    readonly property bool busy: m.state === "requested" || m.state === "verify" || m.state === "approving"

    onCurrentChanged: if (current && live) surface.forceActiveFocus()
    onLiveChanged: if (current && live) surface.forceActiveFocus()

    // Everything but the live picture: a phone outline that shows what's
    // happening, a couple of words, and one round button.
    Column {
        visible: !page.live
        anchors.centerIn: parent
        spacing: Theme.s5

        Rectangle {
            id: outline
            anchors.horizontalCenter: parent.horizontalCenter
            width: 150; height: 280
            radius: 26
            color: Theme.surface
            border.width: 2
            border.color: page.m.state === "verify" ? Theme.accent : Theme.line
            Behavior on border.color { ColorAnimation { duration: Theme.smooth } }
            Rectangle { width: 8; height: 8; radius: 4; color: Theme.background; anchors { top: parent.top; topMargin: 12; horizontalCenter: parent.horizontalCenter } }

            Icon {
                anchors.centerIn: parent
                visible: page.m.state !== "verify" && page.m.state !== "requested"
                name: !page.m.available ? "wifi" : page.m.state === "approving" ? "check" : page.m.state === "ended" ? "x" : "screen"
                size: 46
                color: !page.m.available ? Theme.tertiary : page.m.state === "ended" ? Theme.secondary : Theme.accent
            }
            ProgressRing { anchors.centerIn: parent; visible: page.m.state === "requested"; width: 54; height: 54; thickness: 4; value: -1 }
            Text {
                anchors.centerIn: parent
                visible: page.m.state === "verify"
                text: page.m.icon
                font.pixelSize: 70
                textFormat: Text.PlainText
                Accessible.name: page.m.icon_name
                SequentialAnimation on scale {
                    running: page.m.state === "verify"; loops: Animation.Infinite
                    NumberAnimation { from: 1; to: 1.1; duration: 700; easing.type: Easing.InOutSine }
                    NumberAnimation { from: 1.1; to: 1; duration: 700; easing.type: Easing.InOutSine }
                }
            }
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: !page.m.available ? qsTr("Pair over the network first")
                : ({
                    "requested": qsTr("Waiting for your phone"),
                    "verify": qsTr("Tap the %1 on your phone").arg(page.m.icon_name),
                    "approving": qsTr("Allow it on your phone"),
                    "ended": qsTr("Sharing ended")
                })[page.m.state] || qsTr("Share your screen")
            color: Theme.foreground
            font.family: Theme.font; font.pixelSize: Theme.title; font.weight: Font.DemiBold
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.s3
            Btn { visible: !page.busy && page.m.available; icon: "play"; iconSize: 24; kind: "primary"; round: true; implicitHeight: 64
                  hint: qsTr("Share screen  ·  the phone checks with an icon first"); onClicked: Store.send("mirror_start") }
            Btn { visible: !page.m.available; icon: "gear"; iconSize: 22; kind: "primary"; round: true; implicitHeight: 64
                  hint: qsTr("Pair a phone"); onClicked: page.openPhone() }
            Btn { visible: page.busy; icon: "x"; iconSize: 22; kind: "soft"; round: true; implicitHeight: 64
                  hint: qsTr("Cancel"); onClicked: Store.send("mirror_stop") }
            Btn { visible: page.m.state === "ended" && page.m.reason !== ""; icon: "info"; iconSize: 18; kind: "ghost"; tint: Theme.tertiary
                  round: true; implicitHeight: 64; hint: page.m.reason }
        }
    }

    // The live picture.
    Item {
        anchors.fill: parent
        visible: page.live

        Row {
            id: bar
            anchors { top: parent.top; topMargin: 12; horizontalCenter: parent.horizontalCenter }
            spacing: 6
            Btn { icon: "chevron"; iconSize: 15; rotation: 180; hint: qsTr("Back"); enabled: page.m.control; onClicked: Store.send("mirror_action", { action: "back" }) }
            Btn { text: qsTr("Home"); enabled: page.m.control; onClicked: Store.send("mirror_action", { action: "home" }) }
            Btn { text: qsTr("Apps"); enabled: page.m.control; onClicked: Store.send("mirror_action", { action: "recents" }) }
            Btn { icon: "bell"; iconSize: 15; hint: qsTr("Notifications"); enabled: page.m.control; onClicked: Store.send("mirror_action", { action: "notifications" }) }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                leftPadding: 8; rightPadding: 8
                text: qsTr("%1 fps").arg(page.m.fps) + (page.m.route === "relay" ? "  ·  " + qsTr("relay") : "")
                color: Theme.tertiary
                font.family: Theme.font; font.pixelSize: 12
            }
            Btn { text: qsTr("Stop"); kind: "danger"; onClicked: Store.send("mirror_stop") }
        }

        Text {
            id: viewOnly
            visible: !page.m.control
            anchors { top: bar.bottom; topMargin: 8; horizontalCenter: parent.horizontalCenter }
            width: Math.min(parent.width - 40, 520)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: qsTr("View only. To use the phone from here, turn on Omalink under Accessibility on the phone.")
            color: Theme.yellow
            font.family: Theme.font; font.pixelSize: 12
        }

        VideoOutput {
            id: video
            objectName: "omalinkMirror"
            anchors { left: parent.left; right: parent.right; top: viewOnly.visible ? viewOnly.bottom : bar.bottom; bottom: parent.bottom; margins: 14 }
            fillMode: VideoOutput.PreserveAspectFit
        }

        // Where the pointer is, as a share of the phone's screen.
        Item {
            id: surface
            x: video.x + video.contentRect.x; y: video.y + video.contentRect.y
            width: video.contentRect.width; height: video.contentRect.height
            focus: true
            activeFocusOnTab: true

            function at(mouse) { return { x: mouse.x / Math.max(1, width), y: mouse.y / Math.max(1, height) } }

            MouseArea {
                anchors.fill: parent
                enabled: page.m.control
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                cursorShape: page.m.control ? Qt.PointingHandCursor : Qt.ArrowCursor
                onPressed: mouse => {
                    surface.forceActiveFocus()
                    if (mouse.button === Qt.RightButton) return Store.send("mirror_action", { action: "back" })
                    const p = surface.at(mouse); Store.send("mirror_input", { kind: "down", x: p.x, y: p.y })
                }
                onPositionChanged: mouse => {
                    if (!(mouse.buttons & Qt.LeftButton)) return
                    const p = surface.at(mouse); Store.send("mirror_input", { kind: "move", x: p.x, y: p.y })
                }
                onReleased: mouse => {
                    if (mouse.button !== Qt.LeftButton) return
                    const p = surface.at(mouse); Store.send("mirror_input", { kind: "up", x: p.x, y: p.y })
                }
                onWheel: wheel => {
                    const p = surface.at(wheel)
                    Store.send("mirror_scroll", { x: p.x, y: p.y, dx: -wheel.angleDelta.x / 120, dy: wheel.angleDelta.y / 120 })
                }
            }

            Keys.onPressed: event => {
                if (!page.m.control) return
                const keys = { [Qt.Key_Return]: "enter", [Qt.Key_Enter]: "enter", [Qt.Key_Backspace]: "backspace",
                               [Qt.Key_Delete]: "delete", [Qt.Key_Left]: "left", [Qt.Key_Right]: "right" }
                if (event.key === Qt.Key_Escape) Store.send("mirror_action", { action: "back" })
                else if (keys[event.key]) Store.send("mirror_key", { key: keys[event.key] })
                else if (event.text && !(event.modifiers & Qt.ControlModifier) && event.text.charCodeAt(0) >= 32) Store.send("mirror_text", { text: event.text })
                else return
                event.accepted = true
            }
        }
    }
}
