import QtQuick
import QtQuick.Shapes
import omalink

// Home: the phone itself, on a stage. Its wallpaper is its screen, a glowing
// line runs to this computer with dots flowing along it the way the bytes go,
// and everything you can do sits around it as a bubble. Words only on hover.
Item {
    id: home
    property bool current: false
    signal go(string page)
    signal chooseFiles()

    readonly property bool online: Store.connected
    readonly property string route: Store.link.via === "network" ? (Store.net.route === "relay" ? "relay" : "direct") : Store.link.via === "bluetooth" ? "bluetooth" : ""
    readonly property color linkColor: !online ? Theme.tertiary : route === "relay" ? Theme.yellow : route === "bluetooth" ? Theme.green : Theme.accent
    readonly property int battery: Store.status && online ? Store.status.battery : -1
    readonly property var recent: Store.transfers.slice(0, 6)
    readonly property var apps: {
        const seen = []
        for (const n of Store.notifications) {
            if (seen.length >= 4) break
            if (!seen.some(s => s.package === n.package)) seen.push(n)
        }
        return seen
    }

    // A soft glow behind the phone, in the theme's accent.
    Shape {
        anchors.fill: parent
        opacity: home.online ? 1 : 0.4
        Behavior on opacity { NumberAnimation { duration: Theme.smooth } }
        ShapePath {
            strokeWidth: -1
            fillGradient: RadialGradient {
                centerX: phone.x + phone.width / 2; centerY: phone.y + phone.height / 2
                focalX: centerX; focalY: centerY
                centerRadius: phone.height * 0.95
                GradientStop { position: 0; color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.16) }
                GradientStop { position: 1; color: "transparent" }
            }
            PathRectangle { x: 0; y: 0; width: home.width; height: home.height }
        }
    }

    // This computer.
    Bubble {
        id: computer
        size: 92
        x: Math.max(Theme.s6, phone.x - width - Math.min(260, home.width * 0.16))
        y: phone.y + phone.height / 2 - height / 2
        icon: "laptop"; iconSize: 38
        tint: Theme.foreground
        tip: Store.net.name || qsTr("This computer")
        onClicked: home.go("phone")
    }

    // The link between them.
    Item {
        id: link
        x: computer.x + computer.width + 6
        width: phone.x - x - 6
        y: computer.y + computer.height / 2
        height: 1

        Shape {
            anchors.fill: parent
            ShapePath {
                strokeColor: Qt.rgba(home.linkColor.r, home.linkColor.g, home.linkColor.b, home.online ? 0.55 : 0.35)
                strokeWidth: 2
                strokeStyle: home.online ? ShapePath.SolidLine : ShapePath.DashLine
                dashPattern: [3, 4]
                startX: 0; startY: 0
                PathLine { x: link.width; y: 0 }
            }
        }
        // Bytes on their way: three dots, one after another.
        Repeater {
            model: home.online ? 3 : 0
            Rectangle {
                id: dot
                required property int index
                width: 8; height: 8; radius: 4
                y: -4
                color: home.linkColor
                opacity: 0
                SequentialAnimation {
                    running: home.current && home.online
                    loops: Animation.Infinite
                    PauseAnimation { duration: dot.index * 520 }
                    ParallelAnimation {
                        NumberAnimation { target: dot; property: "x"; from: 0; to: link.width - 8; duration: 1560; easing.type: Easing.InOutSine }
                        SequentialAnimation {
                            NumberAnimation { target: dot; property: "opacity"; from: 0; to: 1; duration: 300 }
                            PauseAnimation { duration: 960 }
                            NumberAnimation { target: dot; property: "opacity"; from: 1; to: 0; duration: 300 }
                        }
                    }
                    PauseAnimation { duration: (2 - dot.index) * 520 }
                }
            }
        }
        // How they're connected, as one icon in the middle of the line.
        Bubble {
            size: 40
            x: (link.width - width) / 2; y: -height / 2
            icon: !home.online ? "x" : home.route === "bluetooth" ? "bluetooth" : home.route === "relay" ? "globe" : "wifi"
            iconSize: 18
            tint: home.linkColor
            tip: !home.online ? (Store.link.state === "none" ? qsTr("No phone yet") : qsTr("Not connected"))
                 : home.route === "bluetooth" ? qsTr("Bluetooth")
                 : home.route === "relay" ? qsTr("Internet, through a relay")
                 : qsTr("Straight across the network")
            onClicked: home.go("phone")
        }
    }

    // The phone, floating a little.
    Item {
        id: phone
        readonly property real h: Math.min(home.height - 190, 560, home.width * 0.62)
        width: Math.round(h * 0.49); height: h
        x: Math.round(home.width * 0.5 - width / 2)
        y: Math.round((home.height - height) / 2) - 34 + float.offset

        QtObject { id: float; property real offset: 0 }
        SequentialAnimation {
            running: home.current
            loops: Animation.Infinite
            NumberAnimation { target: float; property: "offset"; from: 0; to: -6; duration: 2600; easing.type: Easing.InOutSine }
            NumberAnimation { target: float; property: "offset"; from: -6; to: 0; duration: 2600; easing.type: Easing.InOutSine }
        }

        // The body.
        Rectangle {
            anchors.fill: parent
            radius: width * 0.14
            color: "#0b0c0f"
            border.width: 2; border.color: Qt.rgba(1, 1, 1, 0.10)
        }
        // The screen: the phone's own wallpaper.
        Rectangle {
            id: screen
            anchors { fill: parent; margins: 7 }
            radius: parent.width * 0.12
            color: Theme.surface
            clip: true
            Picture { anchors.fill: parent; radius: screen.radius; source: Store.wallpaper; opacity: home.online ? 1 : 0.35
                      Behavior on opacity { NumberAnimation { duration: Theme.smooth } } }
            Icon { visible: Store.wallpaper === ""; anchors.centerIn: parent; name: "device"; size: 48; color: Theme.tertiary }
        }
        // The camera.
        Rectangle { width: 10; height: 10; radius: 5; color: "#000"; anchors { top: parent.top; topMargin: 16; horizontalCenter: parent.horizontalCenter } }

        // The time, big, as a lock screen shows it.
        Text {
            id: clock
            anchors { top: parent.top; topMargin: parent.height * 0.13; horizontalCenter: parent.horizontalCenter }
            // Store.minute ticks, so the time does too.
            text: { Store.minute; return Qt.formatTime(new Date(), "HH:mm") }
            color: "white"
            font.family: Theme.font; font.pixelSize: phone.width * 0.24; font.weight: Font.Light
            style: Text.Raised; styleColor: Qt.rgba(0, 0, 0, 0.35)
            opacity: home.online ? 0.95 : 0.4
        }

        // Who's been talking: app icons from the latest notifications.
        Rectangle {
            visible: home.apps.length > 0 && home.online
            anchors { bottom: parent.bottom; bottomMargin: parent.height * 0.1; horizontalCenter: parent.horizontalCenter }
            width: appRow.width + 18; height: 44; radius: 22
            color: Qt.rgba(0, 0, 0, 0.45)
            border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.12)
            HoverHandler { id: appsHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: home.go("notifications") }
            Row {
                id: appRow
                anchors.centerIn: parent
                spacing: -6
                Repeater {
                    model: home.apps
                    Rectangle {
                        required property var modelData
                        width: 30; height: 30; radius: 15
                        color: "#1b1c20"; border.width: 2; border.color: "#0b0c0f"
                        Image { anchors { fill: parent; margins: 5 } visible: modelData.icon !== ""; source: modelData.icon; sourceSize.width: 40; fillMode: Image.PreserveAspectFit; smooth: true }
                        // No icon: the app's initial.
                        Text { anchors.centerIn: parent; visible: modelData.icon === ""; text: (modelData.app || "?").charAt(0).toUpperCase()
                               color: "white"; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.DemiBold }
                    }
                }
                Text {
                    leftPadding: 12; rightPadding: 2
                    anchors.verticalCenter: parent.verticalCenter
                    text: Store.notifications.length
                    color: "white"
                    font.family: Theme.font; font.pixelSize: Theme.label; font.weight: Font.DemiBold
                }
            }
            Tip { visible: appsHover.hovered; text: qsTr("%n notification(s)", "", Store.notifications.length) }
        }

        // Battery, as a ring hanging off the corner.
        Item {
            visible: home.battery >= 0
            width: 58; height: 58
            x: -width / 2 + 6; y: -12
            Rectangle { anchors.fill: parent; radius: width / 2; color: Theme.surface; border.width: 1; border.color: Theme.line }
            ProgressRing {
                anchors { fill: parent; margins: 4 }
                thickness: 4
                value: home.battery / 100
                color: Store.status && Store.status.charging ? Theme.green : home.battery <= 15 ? Theme.red : Theme.accent
            }
            Text {
                anchors.centerIn: parent
                text: home.battery
                color: Theme.foreground
                font.family: Theme.font; font.pixelSize: 15; font.weight: Font.DemiBold
            }
            Icon { visible: Store.status && Store.status.charging; name: "bolt"; size: 12; color: Theme.green
                   anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 6 } }
            HoverHandler { id: batteryHover }
            Tip { visible: batteryHover.hovered; text: qsTr("Battery %1%").arg(home.battery) + (Store.status && Store.status.charging ? qsTr(", charging") : "") }
        }

    }

    // What you can do, around the phone.
    Repeater {
        id: actions
        model: [
            { icon: "upload", tip: qsTr("Send files"), act: "files" },
            { icon: "screen", tip: qsTr("Share screen"), act: "screen" },
            { icon: "clipboard", tip: qsTr("Clipboard"), act: "clipboard" },
            { icon: "message", tip: qsTr("Messages"), act: "messages", badge: Store.unread },
            { icon: "phone", tip: qsTr("Calls"), act: "calls" },
            { icon: "ring", tip: qsTr("Ring the phone"), act: "find" }
        ]
        Bubble {
            id: action
            required property var modelData
            required property int index
            // On a stretched arc to the phone's right, never over the phone.
            readonly property real angle: (-60 + index * (120 / (actions.count - 1))) * Math.PI / 180
            size: Math.round(Math.min(64, phone.height * 0.115))
            x: phone.x + phone.width + 34 + (Math.cos(angle) - 0.5) * 2 * Math.min(70, home.width * 0.05)
            y: phone.y + phone.height / 2 + Math.sin(angle) * phone.height * 0.42 - height / 2
            icon: modelData.icon; iconSize: 24
            tint: Theme.accent
            tip: modelData.tip
            badge: modelData.badge || 0
            enabled: home.online || modelData.act === "files"
            opacity: enabled ? 1 : 0.35
            onClicked: {
                switch (modelData.act) {
                case "files": home.chooseFiles(); break
                case "screen": home.go("screen"); Store.send("mirror_start"); break
                case "find": Store.send("find_phone"); action.wiggle(); break
                default: home.go(modelData.act)
                }
            }
        }
    }

    // Recent files, as a strip of pictures under it all.
    Row {
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: Theme.s5 }
        spacing: Theme.s2
        visible: home.recent.length > 0
        Repeater {
            model: home.recent
            Item {
                id: mini
                required property var modelData
                width: 56; height: 56
                HoverHandler { id: miniHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: home.go("files") }
                Thumb {
                    anchors.fill: parent
                    radius: Theme.card
                    source: mini.modelData.thumb; kind: mini.modelData.kind; glyphSize: 20
                    scale: miniHover.hovered ? 1.12 : 1
                    Behavior on scale { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutBack } }
                    border.width: 1; border.color: Theme.line
                }
                Rectangle {
                    width: 18; height: 18; radius: 9
                    anchors { right: parent.right; bottom: parent.bottom; margins: -3 }
                    color: Theme.surface; border.width: 1; border.color: Theme.line
                    Icon { anchors.centerIn: parent; name: mini.modelData.incoming ? "in" : "out"; size: 11; weight: 2
                           color: mini.modelData.state === "failed" ? Theme.red : Theme.accent }
                }
                Tip { visible: miniHover.hovered; text: mini.modelData.name }
            }
        }
    }

    // Nothing to show yet: one big way forward.
    Bubble {
        visible: Store.link.state === "none" && Store.net.phones.length === 0
        size: 76
        anchors { horizontalCenter: phone.horizontalCenter; top: phone.bottom; topMargin: Theme.s5 }
        icon: "plus"; iconSize: 30
        tint: Theme.onAccent
        filled: true
        tip: qsTr("Pair a phone")
        onClicked: home.go("phone")
    }

    // A round button that springs under the pointer.
    component Bubble: Item {
        id: bubble
        property real size: 64
        property string icon: ""
        property real iconSize: 24
        property color tint: Theme.accent
        property string tip: ""
        property int badge: 0
        property bool filled: false
        signal clicked()
        function wiggle() { shake.restart() }

        width: size; height: size

        Rectangle {
            id: face
            anchors.fill: parent
            radius: width / 2
            color: bubble.filled ? Theme.accent : hover.hovered ? Theme.raised : Theme.surface
            border.width: 1
            border.color: hover.hovered ? Qt.rgba(bubble.tint.r, bubble.tint.g, bubble.tint.b, 0.6) : Theme.line
            scale: tap.pressed ? 0.92 : hover.hovered ? 1.12 : 1
            Behavior on scale { NumberAnimation { duration: Theme.smooth; easing.type: Easing.OutBack } }
            Behavior on color { ColorAnimation { duration: Theme.quick } }
            Icon { id: glyph; anchors.centerIn: parent; name: bubble.icon; size: bubble.iconSize; color: bubble.tint }
        }
        SequentialAnimation {
            id: shake
            loops: 3
            NumberAnimation { target: glyph; property: "rotation"; to: -18; duration: 70 }
            NumberAnimation { target: glyph; property: "rotation"; to: 18; duration: 110 }
            NumberAnimation { target: glyph; property: "rotation"; to: 0; duration: 70 }
        }
        Rectangle {
            visible: bubble.badge > 0
            anchors { right: parent.right; top: parent.top; margins: -2 }
            height: 20; width: Math.max(20, badgeText.implicitWidth + 10); radius: 10
            color: Theme.accent
            border.width: 2; border.color: Theme.background
            Text { id: badgeText; anchors.centerIn: parent; text: bubble.badge > 99 ? "99+" : bubble.badge
                   color: Theme.onAccent; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Bold }
        }
        HoverHandler { id: hover; cursorShape: bubble.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
        TapHandler { id: tap; onTapped: bubble.clicked() }
        Tip { visible: hover.hovered && bubble.tip !== ""; text: bubble.tip }
    }
}
