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
    readonly property var order: ["home", "messages", "rcs", "calls", "notifications", "clipboard", "files", "screen", "phone"]
    function go(name) { const i = order.indexOf(name); if (i >= 0) page = i }
    // In the rail's order. `bottom` ones sit at the foot of the rail; `hot`
    // badges are in the accent, the rest quieter.
    readonly property var pages: [
        { icon: "home", label: qsTr("Home"), key: "Ctrl+1", badge: 0 },
        { icon: "message", label: qsTr("Messages"), key: "Ctrl+2", badge: Store.unread, hot: true },
        { icon: "chat", label: qsTr("RCS chats"), key: "Ctrl+3", badge: 0 },
        { icon: "phone", label: qsTr("Calls"), key: "Ctrl+4", badge: 0 },
        { icon: "bell", label: qsTr("Notifications"), key: "Ctrl+5", badge: Store.notifications.length },
        { icon: "clipboard", label: qsTr("Clipboard"), key: "Ctrl+6", badge: 0 },
        { icon: "image", label: qsTr("Files"), key: "Ctrl+7", badge: Store.transfers.filter(t => t.state === "active" || t.state === "waiting").length, hot: true },
        { icon: "screen", label: qsTr("Screen"), key: "Ctrl+8", badge: 0 },
        { icon: "gear", label: qsTr("Phone and settings"), key: "Ctrl+9", badge: Store.connected ? Store.missing.length : 0, bottom: true }
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
        function onShot(path, page, delay, width, height) {
            if (width > 0) win.width = width
            if (height > 0) win.height = height
            shooter.interval = delay
            win.page = typeof page === "string" ? Math.max(0, win.order.indexOf(page)) : page
            if (win.order[win.page] === "messages" && Store.conversations.length) messagesPage.open(Store.conversations[0])
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

    Repeater {
        model: 9
        Item {
            required property int index
            Shortcut { sequence: "Ctrl+" + (index + 1); context: Qt.ApplicationShortcut; onActivated: win.page = index }
        }
    }
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

        Rail {
            id: sidebar
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            pages: win.pages
            current: win.page
            onPick: index => win.page = index
        }

        // Pages cross-fade and rise a little as they come in.
        Item {
            id: stage
            anchors { left: sidebar.right; right: parent.right; top: parent.top; bottom: parent.bottom }
            clip: true

            component Page: Item {
                property string name: ""
                readonly property int index: win.order.indexOf(name)
                readonly property bool current: win.page === index
                anchors.fill: parent
                visible: opacity > 0
                opacity: current ? 1 : 0
                enabled: current
                transform: Translate { y: current ? 0 : 10; Behavior on y { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutCubic } } }
                Behavior on opacity { NumberAnimation { duration: 180 } }
            }

            Page { name: "home"; HomePage { anchors.fill: parent; current: parent.current
                                            onGo: name => win.go(name)
                                            onChooseFiles: filesPage.choose() } }
            Page { name: "messages"; MessagesPage { id: messagesPage; anchors.fill: parent; onSetup: win.go("phone") } }
            Page { name: "rcs"; RcsPage { anchors.fill: parent; current: parent.current } }
            Page { name: "calls"; CallsPage { id: callsPage; anchors.fill: parent; onSetup: win.go("phone")
                                         onMessage: number => {
                                             win.go("messages")
                                             const key = Store.numberKey(number)
                                             const existing = Store.conversations.find(c => Store.numberKey(c.address) === key)
                                             if (existing) messagesPage.open(existing); else messagesPage.compose(number)
                                         } } }
            Page { name: "notifications"; NotificationsPage { anchors.fill: parent; onSetup: win.go("phone") } }
            Page { name: "clipboard"; ClipboardPage { anchors.fill: parent; onSetup: win.go("phone") } }
            Page { name: "files"; FilesPage { id: filesPage; anchors.fill: parent; current: parent.current; onSetup: win.go("phone") } }
            Page { name: "phone"; PhonePage { anchors.fill: parent; onConfirmForget: device => confirm.device = device } }
            Page { name: "screen"; MirrorPage { anchors.fill: parent; current: parent.current; onOpenPhone: win.go("phone") } }

            // A file dropped on any page goes to the phone.
            // Files dragged in from elsewhere; a card dragged out of the
            // Files page is the app's own and isn't sent back.
            DropArea {
                id: drops
                anchors.fill: parent
                keys: ["text/uri-list"]
                onEntered: drag => { if (drag.source) drag.accepted = false }
                onDropped: drop => {
                    if (drop.source || !Store.connected) return
                    filesPage.send(drop.urls)
                    win.go("files")
                }
            }
            DropOverlay {
                anchors.fill: parent
                z: 7
                active: drops.containsDrag || Store.demoDrop
                accepting: Store.connected
                target: Store.link.name
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
                            ["Ctrl+1 … 9", qsTr("Home, Messages, RCS chats, Calls, Notifications, Clipboard, Files, Screen, Phone")],
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
