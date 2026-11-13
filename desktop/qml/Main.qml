import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import omalink

ApplicationWindow {
    id: win
    width: 1080
    height: 700
    minimumWidth: 860
    minimumHeight: 540
    visible: false
    title: "Omalink"
    color: Theme.background
    font.family: Theme.font

    property int page: 0
    property bool helpVisible: false
    // Pages are addressed by name everywhere but the sidebar, so adding one
    // doesn't renumber the rest.
    readonly property var order: ["messages", "rcs", "calls", "notifications", "clipboard", "files", "phone"]
    function go(name) { const i = order.indexOf(name); if (i >= 0) page = i }
    readonly property var pages: [
        { icon: "message", label: qsTr("Messages"), badge: Store.unread },
        { icon: "chat", label: qsTr("RCS chats"), badge: 0 },
        { icon: "phone", label: qsTr("Calls"), badge: 0 },
        { icon: "bell", label: qsTr("Notifications"), badge: Store.notifications.length },
        { icon: "clipboard", label: qsTr("Clipboard"), badge: 0 },
        { icon: "file", label: qsTr("Files"), badge: Store.transfers.filter(t => t.state === "active" || t.state === "waiting").length },
        { icon: "device", label: qsTr("Phone"), badge: Store.connected ? Store.missing.length : 0 }
    ]

    // Closing the window keeps the link alive, the way a phone companion
    // should; Ctrl+Q really quits.
    // Quitting closes every window first, so the handler has to know the
    // difference or it would cancel the quit.
    property bool quitting: false
    onClosing: close => { if (!quitting) { close.accepted = false; win.hide() } }
    function quit() { quitting = true; Store.send("bye"); farewell.start() }
    // A moment for the core to mark itself as stopped before the process goes.
    Timer { id: farewell; interval: 60; onTriggered: Qt.quit() }

    function reveal() { win.show(); win.raise(); win.requestActivate() }

    Backend {
        id: backend
        onUpdate: (topic, payload) => Store.apply(topic, JSON.parse(payload))
        Component.onCompleted: { Store.backend = backend; start() }
    }

    Connections {
        target: Store
        function onReady(visible) {
            // With no phone linked the first thing to do is link one.
            if (Store.link.state === "none") win.go("phone")
            if (visible) win.reveal()
        }
        function onShow(name) {
            win.go(name)
            win.reveal()
        }
        function onNote(text) { toast.say(text) }
        function onQuit() { win.quit() }
        function onShot(path, page, delay) {
            shooter.interval = delay
            win.page = page
            if (page === 0 && Store.conversations.length) messagesPage.open(Store.conversations[0])
            shooter.path = path
            shooter.start()
        }
        function onCallChanged() { if (Store.call.phase === "incoming") win.reveal() }
    }

    Timer {
        id: shooter
        property string path: ""
        interval: 900
        onTriggered: root.grabToImage(result => { result.saveToFile(shooter.path); win.quit() })
    }

    Shortcut { sequence: "Ctrl+1"; context: Qt.ApplicationShortcut; onActivated: win.page = 0 }
    Shortcut { sequence: "Ctrl+2"; context: Qt.ApplicationShortcut; onActivated: win.page = 1 }
    Shortcut { sequence: "Ctrl+3"; context: Qt.ApplicationShortcut; onActivated: win.page = 2 }
    Shortcut { sequence: "Ctrl+4"; context: Qt.ApplicationShortcut; onActivated: win.page = 3 }
    Shortcut { sequence: "Ctrl+5"; context: Qt.ApplicationShortcut; onActivated: win.page = 4 }
    Shortcut { sequence: "Ctrl+6"; context: Qt.ApplicationShortcut; onActivated: win.page = 5 }
    Shortcut { sequence: "Ctrl+7"; context: Qt.ApplicationShortcut; onActivated: win.page = 6 }
    Shortcut { sequence: "Ctrl+N"; context: Qt.ApplicationShortcut; onActivated: { win.go("messages"); messagesPage.compose("") } }
    Shortcut { sequence: "Ctrl+F"; context: Qt.ApplicationShortcut; onActivated: { win.go("messages"); messagesPage.focusSearch() } }
    Shortcut { sequence: "Ctrl+D"; context: Qt.ApplicationShortcut; onActivated: { win.go("calls"); callsPage.focusDial() } }
    Shortcut { sequence: "Ctrl+R"; context: Qt.ApplicationShortcut; onActivated: Store.send("sync") }
    Shortcut { sequence: "Ctrl+Return"; context: Qt.ApplicationShortcut; enabled: Store.call.phase === "incoming"; onActivated: Store.send("answer") }
    Shortcut { sequence: "Ctrl+."; context: Qt.ApplicationShortcut; enabled: Store.call.phase !== "idle"; onActivated: Store.send("hangup") }
    Shortcut { sequence: "?"; context: Qt.ApplicationShortcut; onActivated: win.helpVisible = !win.helpVisible }
    Shortcut { sequence: "Escape"; context: Qt.ApplicationShortcut; enabled: win.helpVisible; onActivated: win.helpVisible = false }
    Shortcut { sequence: "Ctrl+W"; context: Qt.ApplicationShortcut; onActivated: win.hide() }
    Shortcut { sequence: "Ctrl+Q"; context: Qt.ApplicationShortcut; onActivated: win.quit() }

    // Right-to-left languages mirror the whole window.
    Item {
        id: root
        anchors.fill: parent
        LayoutMirroring.enabled: Qt.application.layoutDirection === Qt.RightToLeft
        LayoutMirroring.childrenInherit: true

        Rectangle { anchors.fill: parent; color: Theme.background }

        Rectangle {
            id: sidebar
            width: 220
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            color: Theme.sidebar

            // The phone, at a glance.
            Item {
                id: phone
                height: 72
                anchors { left: parent.left; right: parent.right; top: parent.top; topMargin: 8 }
                Rectangle {
                    anchors { fill: parent; leftMargin: 8; rightMargin: 8 }
                    radius: Theme.small
                    color: phoneArea.containsMouse ? Theme.hover : "transparent"
                    Behavior on color { ColorAnimation { duration: Theme.quick } }
                }
                Rectangle {
                    id: phoneIcon
                    width: 38; height: 38; radius: Theme.radius > 0 ? 19 : 0
                    color: Store.connected ? Theme.accentSoft : Theme.hover
                    anchors { left: parent.left; leftMargin: 18; verticalCenter: parent.verticalCenter }
                    Icon { visible: !wallpaper.ready; anchors.centerIn: parent; name: "device"; size: 19; color: Store.connected ? Theme.accent : Theme.tertiary }
                    Picture { id: wallpaper; anchors.fill: parent; radius: parent.radius; source: Store.wallpaper; opacity: Store.connected ? 1 : 0.5 }
                    Rectangle {
                        width: 10; height: 10; radius: 5
                        anchors { right: parent.right; bottom: parent.bottom; margins: -1 }
                        color: Store.connected ? Theme.green : Store.link.state === "connecting" ? Theme.yellow : Theme.tertiary
                        border.width: 2; border.color: Theme.sidebar
                    }
                }
                Column {
                    anchors { left: phoneIcon.right; leftMargin: 11; right: parent.right; rightMargin: 16; verticalCenter: parent.verticalCenter }
                    spacing: 3
                    Text {
                        width: parent.width; elide: Text.ElideRight
                        text: Store.link.state === "none" ? qsTr("No phone") : (Store.link.name || qsTr("Phone"))
                        color: Theme.foreground
                        font.family: Theme.font; font.pixelSize: 14; font.weight: Font.DemiBold
                    }
                    Row {
                        spacing: 6
                        visible: Store.connected && Store.status
                        // Battery, drawn: an outline that fills with charge.
                        Item {
                            width: 22; height: 11
                            anchors.verticalCenter: parent.verticalCenter
                            readonly property int level: Store.status ? Store.status.battery : 0
                            Rectangle { width: 19; height: 11; radius: 2.5; color: "transparent"; border.width: 1; border.color: Theme.tertiary
                                Rectangle { x: 2; y: 2; height: 7; radius: 1
                                    width: Math.max(1, 15 * Math.max(0, parent.parent.level) / 100)
                                    color: Store.status && Store.status.charging ? Theme.green : parent.parent.level <= 15 ? Theme.red : Theme.secondary } }
                            Rectangle { x: 20; y: 3.5; width: 2; height: 4; radius: 1; color: Theme.tertiary }
                        }
                        Text { text: Store.status && Store.status.battery >= 0 ? Store.status.battery + "%" : ""; color: Theme.secondary; font.family: Theme.font; font.pixelSize: 12; anchors.verticalCenter: parent.verticalCenter }
                        Icon { visible: Store.status && Store.status.charging; name: "bolt"; size: 11; color: Theme.green; anchors.verticalCenter: parent.verticalCenter }
                        // Signal bars.
                        Row {
                            visible: Store.status && Store.status.signal >= 0
                            spacing: 1.5
                            anchors.verticalCenter: parent.verticalCenter
                            Repeater {
                                model: 4
                                Rectangle { required property int index; width: 3; height: 4 + index * 2.3; radius: 1; anchors.bottom: parent.bottom
                                            color: Store.status && index < Store.status.signal ? Theme.secondary : Theme.pressed }
                            }
                        }
                    }
                    Text {
                        visible: !(Store.connected && Store.status)
                        width: parent.width; elide: Text.ElideRight
                        text: ({ "connected": qsTr("Connected"), "connecting": qsTr("Connecting…"), "waiting": qsTr("Not connected"), "paused": qsTr("Disconnected"), "none": qsTr("Set up") })[Store.link.state] || ""
                        color: Theme.secondary
                        font.family: Theme.font; font.pixelSize: 12
                    }
                }
                MouseArea { id: phoneArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.go("phone") }
            }

            // The marker glides between items rather than jumping.
            Rectangle {
                z: 2
                x: 8; width: 3; height: 18; radius: Math.min(1.5, Theme.radius)
                y: nav.y + win.page * 40 + 10
                color: Theme.accent
                Behavior on y { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } }
            }

            Column {
                id: nav
                anchors { left: parent.left; right: parent.right; top: phone.bottom; topMargin: 10 }
                spacing: 2

                Repeater {
                    model: win.pages
                    Item {
                        id: item
                        required property var modelData
                        required property int index
                        readonly property bool current: win.page === index
                        width: nav.width; height: 38

                        Rectangle {
                            anchors { fill: parent; leftMargin: 8; rightMargin: 8 }
                            radius: Theme.small
                            color: item.current ? Theme.hover : area.containsMouse ? Theme.hover : "transparent"
                            Behavior on color { ColorAnimation { duration: Theme.quick } }
                        }
                        Icon { id: glyph; name: item.modelData.icon; size: 17
                               color: item.current ? Theme.foreground : Theme.secondary
                               anchors { left: parent.left; leftMargin: 24; verticalCenter: parent.verticalCenter } }
                        Text {
                            anchors { left: glyph.right; leftMargin: 12; verticalCenter: parent.verticalCenter }
                            text: item.modelData.label
                            color: item.current ? Theme.foreground : Theme.secondary
                            font.family: Theme.font; font.pixelSize: 14
                            font.weight: item.current ? Font.DemiBold : Font.Normal
                        }
                        Rectangle {
                            visible: item.modelData.badge > 0
                            anchors { right: parent.right; rightMargin: 20; verticalCenter: parent.verticalCenter }
                            height: 18; width: Math.max(18, count.implicitWidth + 10); radius: Theme.radius > 0 ? 9 : 2
                            color: item.index === 0 ? Theme.accent : Theme.pressed
                            Text { id: count; anchors.centerIn: parent; text: item.modelData.badge > 99 ? "99+" : item.modelData.badge
                                   color: item.index === 0 ? Theme.onAccent : Theme.secondary
                                   font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Bold }
                        }
                        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.page = item.index }
                    }
                }
            }
        }

        // Pages cross-fade and rise a little as they come in.
        Item {
            id: stage
            anchors { left: sidebar.right; right: parent.right; top: parent.top; bottom: parent.bottom }
            clip: true

            component Page: Item {
                property int index: 0
                readonly property bool current: win.page === index
                anchors.fill: parent
                visible: opacity > 0
                opacity: current ? 1 : 0
                enabled: current
                transform: Translate { y: current ? 0 : 10; Behavior on y { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } } }
                Behavior on opacity { NumberAnimation { duration: 180 } }
            }

            Page { index: 0; MessagesPage { id: messagesPage; anchors.fill: parent; onSetup: win.go("phone") } }
            Page { index: 1; RcsPage { anchors.fill: parent; current: win.page === 1 } }
            Page { index: 2; CallsPage { id: callsPage; anchors.fill: parent; onSetup: win.go("phone")
                                         onMessage: number => {
                                             win.go("messages")
                                             const key = Store.numberKey(number)
                                             const existing = Store.conversations.find(c => Store.numberKey(c.address) === key)
                                             if (existing) messagesPage.open(existing); else messagesPage.compose(number)
                                         } } }
            Page { index: 3; NotificationsPage { anchors.fill: parent; onSetup: win.go("phone") } }
            Page { index: 4; ClipboardPage { anchors.fill: parent; onSetup: win.go("phone") } }
            Page { index: 5; FilesPage { id: filesPage; anchors.fill: parent; onSetup: win.go("phone") } }
            Page { index: 6; PhonePage { anchors.fill: parent; onConfirmForget: device => confirm.device = device } }

            // A file dropped on any page goes to the phone.
            DropArea {
                anchors.fill: parent
                enabled: Store.connected
                keys: ["text/uri-list"]
                onDropped: drop => { filesPage.send(drop.urls); win.go("files") }
            }

            CallPanel { anchors.horizontalCenter: parent.horizontalCenter; z: 5 }

            // Short messages from the core: what went wrong, what to do.
            Rectangle {
                id: toast
                z: 6
                function say(text) { label.text = text; shown = true; hide.restart() }
                property bool shown: false
                anchors.horizontalCenter: parent.horizontalCenter
                y: shown ? parent.height - height - 22 : parent.height + 10
                opacity: shown ? 1 : 0
                width: Math.min(label.implicitWidth + 36, parent.width - 60)
                height: label.height + 22
                radius: Theme.small
                color: Theme.raised
                border.width: 1; border.color: Theme.line
                Behavior on y { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } }
                Behavior on opacity { NumberAnimation { duration: Theme.smooth } }
                Text { id: label; anchors.centerIn: parent; width: Math.min(implicitWidth, toast.parent.width - 96)
                       wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter
                       color: Theme.foreground; font.family: Theme.font; font.pixelSize: 13 }
                Timer { id: hide; interval: 5200; onTriggered: toast.shown = false }
                TapHandler { onTapped: toast.shown = false }
            }
        }

        // Dialogs share one scrim.
        Rectangle {
            id: scrim
            anchors.fill: parent
            z: 10
            readonly property bool open: Store.pairing !== null || confirm.device !== null || win.helpVisible
            color: Qt.rgba(0, 0, 0, 0.55)
            opacity: open ? 1 : 0
            visible: opacity > 0
            Behavior on opacity { NumberAnimation { duration: Theme.smooth } }
            MouseArea { anchors.fill: parent; hoverEnabled: true; onClicked: win.helpVisible = false; onWheel: wheel => wheel.accepted = true }

            // Pairing: the code to compare with the phone.
            Rectangle {
                visible: Store.pairing !== null
                anchors.centerIn: parent
                width: 380; height: pairBody.height + 48
                radius: Math.max(Theme.small, Math.min(14, Theme.radius))
                color: Theme.surface
                border.width: 1; border.color: Theme.line
                scale: scrim.open ? 1 : 0.96
                Behavior on scale { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } }
                MouseArea { anchors.fill: parent }
                Column {
                    id: pairBody
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 24 }
                    spacing: 14
                    Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight
                           text: Store.pairing && Store.pairing.name ? qsTr("Pairing with %1").arg(Store.pairing.name) : qsTr("Pairing")
                           color: Theme.foreground; font.family: Theme.font; font.pixelSize: 16; font.weight: Font.DemiBold }
                    Text { visible: Store.pairing && Store.pairing.code !== ""; width: parent.width; horizontalAlignment: Text.AlignHCenter
                           text: Store.pairing ? Store.pairing.code.replace(/(\d{3})(\d{3})/, "$1 $2") : ""
                           color: Theme.accent; font.family: Theme.font; font.pixelSize: 34; font.weight: Font.Bold; font.letterSpacing: 3 }
                    Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; lineHeight: 1.25
                           text: !Store.pairing ? "" : Store.pairing.code === "" ? qsTr("Waiting for the phone… Accept the pairing request there.")
                               : Store.pairing.confirm ? qsTr("Check that the phone shows the same code, then confirm on both.")
                               : qsTr("Enter this code on the phone.")
                           color: Theme.secondary; font.family: Theme.font; font.pixelSize: 13 }
                    Row {
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: 8
                        visible: Store.pairing && Store.pairing.confirm
                        Btn { text: qsTr("Cancel"); onClicked: Store.send("pair_answer", { accept: false }) }
                        Btn { text: qsTr("Codes match"); kind: "primary"; onClicked: { Store.send("pair_answer", { accept: true }); Store.pairing = { name: Store.pairing.name, code: "", confirm: false } } }
                    }
                }
            }

            // Forgetting a device.
            Rectangle {
                id: confirm
                property var device: null
                visible: device !== null
                anchors.centerIn: parent
                width: 380; height: forgetBody.height + 48
                radius: Math.max(Theme.small, Math.min(14, Theme.radius))
                color: Theme.surface
                border.width: 1; border.color: Theme.line
                MouseArea { anchors.fill: parent }
                Column {
                    id: forgetBody
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 24 }
                    spacing: 12
                    Text { width: parent.width; wrapMode: Text.WordWrap
                           text: confirm.device ? qsTr("Forget %1?").arg(confirm.device.name || confirm.device.address) : ""
                           color: Theme.foreground; font.family: Theme.font; font.pixelSize: 16; font.weight: Font.DemiBold }
                    Text { width: parent.width; wrapMode: Text.WordWrap; lineHeight: 1.25
                           text: qsTr("This removes the Bluetooth pairing from this computer. You'll need to pair again to reconnect.")
                           color: Theme.secondary; font.family: Theme.font; font.pixelSize: 13 }
                    Row {
                        anchors.right: parent.right
                        spacing: 8
                        Btn { text: qsTr("Cancel"); onClicked: confirm.device = null }
                        Btn { text: qsTr("Forget"); kind: "soft"; tint: Theme.red; onClicked: { Store.send("forget", { path: confirm.device.path }); confirm.device = null } }
                    }
                }
            }

            // Every shortcut, on one card.
            Rectangle {
                visible: win.helpVisible
                anchors.centerIn: parent
                width: 400; height: keys.height + 48
                radius: Math.max(Theme.small, Math.min(14, Theme.radius))
                color: Theme.surface
                border.width: 1; border.color: Theme.line
                MouseArea { anchors.fill: parent }
                Column {
                    id: keys
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 24 }
                    spacing: 9
                    Text { text: qsTr("Keyboard shortcuts"); color: Theme.foreground; font.family: Theme.font; font.pixelSize: 16; font.weight: Font.DemiBold; bottomPadding: 6 }
                    Repeater {
                        model: [
                            ["Ctrl+1 … 7", qsTr("Messages, RCS chats, Calls, Notifications, Clipboard, Files, Phone")],
                            ["Ctrl+N", qsTr("New message")],
                            ["Ctrl+F", qsTr("Search messages")],
                            ["Ctrl+D", qsTr("Dial a number")],
                            ["Enter", qsTr("Send message · Call the number")],
                            ["Shift+Enter", qsTr("New line in a message")],
                            ["Ctrl+Enter", qsTr("Answer an incoming call")],
                            ["Ctrl+.", qsTr("Hang up or decline")],
                            ["Ctrl+R", qsTr("Refresh from the phone")],
                            ["Ctrl+W", qsTr("Close the window, stay connected")],
                            ["Ctrl+Q", qsTr("Quit")],
                            ["?", qsTr("Show or hide this")]
                        ]
                        Row {
                            required property var modelData
                            spacing: 14
                            Text { width: 104; text: modelData[0]; color: Theme.accent; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.DemiBold }
                            Text { width: 234; wrapMode: Text.WordWrap; text: modelData[1]; color: Theme.secondary; font.family: Theme.font; font.pixelSize: 13 }
                        }
                    }
                }
            }
        }
    }
}
