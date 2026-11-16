import QtQuick
import QtQuick.Controls
import omalink

// Everything about the connection itself: pairing, permissions, settings.
Flickable {
    id: page
    contentHeight: column.height + 48
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

    readonly property var linked: Store.devices.find(d => d.address === Store.link.address) || null
    readonly property bool bluetoothOn: Store.adapter && Store.adapter.powered
    readonly property var others: Store.devices.filter(d => d.address !== Store.link.address && (d.paired || d.name !== ""))

    readonly property string stateText: ({
        "connected": qsTr("Connected"),
        "connecting": qsTr("Connecting…"),
        "waiting": qsTr("Not connected"),
        "paused": qsTr("Disconnected"),
        "none": qsTr("No phone linked")
    })[Store.link.state] || ""

    Column {
        id: column
        width: Math.min(parent.width - 40, 680)
        anchors.horizontalCenter: parent.horizontalCenter
        y: 22
        spacing: 22

        // The linked phone.
        Rectangle {
            width: parent.width
            height: summary.height + 36
            radius: Theme.small
            color: Theme.surface

            Row {
                id: summary
                anchors { left: parent.left; leftMargin: 18; right: parent.right; rightMargin: 18; top: parent.top; topMargin: 18 }
                spacing: 16
                // The phone itself, wearing its wallpaper once that has arrived.
                Rectangle {
                    width: 52; height: wallpaper.ready ? 108 : 52
                    radius: Theme.radius > 0 ? (wallpaper.ready ? 9 : 26) : 0
                    color: Store.connected ? Theme.accentSoft : Theme.hover
                    border.width: wallpaper.ready ? 2 : 0; border.color: Theme.pressed
                    anchors.verticalCenter: parent.verticalCenter
                    Icon { visible: !wallpaper.ready; anchors.centerIn: parent; name: "device"; size: 24; color: Store.connected ? Theme.accent : Theme.secondary }
                    Picture { id: wallpaper; anchors { fill: parent; margins: 2 } radius: Math.max(0, parent.radius - 2); source: Store.wallpaper; opacity: Store.connected ? 1 : 0.5 }
                }
                Column {
                    width: parent.width - 68 - buttons.width - 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Text {
                        width: parent.width; elide: Text.ElideRight
                        text: Store.link.state === "none" ? qsTr("Link your phone") : (Store.link.name || Store.link.address)
                        color: Theme.foreground
                        font.family: Theme.font; font.pixelSize: 17; font.weight: Font.DemiBold
                    }
                    Row {
                        spacing: 7
                        Rectangle { width: 7; height: 7; radius: 3.5; anchors.verticalCenter: parent.verticalCenter
                                    color: Store.connected ? Theme.green : Store.link.state === "connecting" ? Theme.yellow : Theme.tertiary }
                        Text {
                            text: Store.link.state === "none" ? qsTr("Pair an Android phone below to get started.")
                                : Store.connected && Store.status && Store.status.battery >= 0
                                  ? page.stateText + "  ·  " + qsTr("Battery %1%").arg(Store.status.battery) + (Store.status.charging ? " " + qsTr("(charging)") : "")
                                  : page.stateText
                            color: Theme.secondary
                            font.family: Theme.font; font.pixelSize: 13
                        }
                    }
                    Text {
                        visible: !Store.connected && Store.link.error !== ""
                        width: parent.width; wrapMode: Text.WordWrap
                        text: Store.link.error
                        color: Theme.yellow
                        font.family: Theme.font; font.pixelSize: 13
                    }
                }
                Row {
                    id: buttons
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    Btn { visible: Store.connected; icon: "ring"; text: qsTr("Find"); hint: qsTr("Ring the phone"); onClicked: Store.send("find_phone") }
                    Btn { visible: Store.connected; text: qsTr("Disconnect"); kind: "ghost"; tint: Theme.secondary; onClicked: Store.send("disconnect") }
                    Btn { visible: !Store.connected && Store.link.state !== "none" && !Store.link.stale; text: qsTr("Connect"); kind: "primary"; enabled: Store.link.state !== "connecting"; onClicked: Store.send("connect") }
                    // The old pairing is dead: drop it here, which leads back to setup.
                    Btn { visible: !Store.connected && !!Store.link.stale && page.linked !== null; text: qsTr("Pair again"); kind: "primary"; onClicked: Store.send("forget", { path: page.linked.path }) }
                }
            }
        }

        // First run: the two things to do.
        Section {
            width: parent.width
            visible: !Store.connected
            title: qsTr("Getting connected")
            SettingRow {
                icon: page.linked && page.linked.paired ? "check" : "bluetooth"
                iconColor: page.linked && page.linked.paired ? Theme.green : Theme.secondary
                title: qsTr("1.  Pair over Bluetooth")
                detail: page.linked && page.linked.paired ? qsTr("Paired with %1.").arg(page.linked.name || page.linked.address)
                      : qsTr("Open Bluetooth settings on the phone so it can be found, then press Scan below and pick it.")
            }
            SettingRow {
                icon: "device"
                title: qsTr("2.  Open Omalink on the phone")
                detail: qsTr("Install the Omalink app, open it once, and allow what it asks. It keeps listening for this computer after that.")
            }
        }

        // Permissions.
        Section {
            width: parent.width
            visible: Store.connected && (Store.granted.length + Store.missing.length) > 0
            title: qsTr("Phone permissions")
            detail: Store.missing.length ? qsTr("Some features are waiting for access on the phone.") : qsTr("Everything is allowed.")
            trailing: Btn {
                visible: Store.missing.length > 1
                text: qsTr("Ask for all"); kind: "primary"; implicitHeight: 30
                onClicked: Store.send("request_permissions", { permissions: Store.missing })
            }
            Repeater {
                model: Object.keys(Store.permissionInfo).filter(p => Store.granted.indexOf(p) >= 0 || Store.missing.indexOf(p) >= 0)
                SettingRow {
                    required property string modelData
                    readonly property bool ok: Store.has(modelData)
                    icon: ok ? "check" : "shield"
                    iconColor: ok ? Theme.green : Theme.yellow
                    title: Store.permissionInfo[modelData][0]
                    detail: Store.permissionInfo[modelData][1]
                    Btn { visible: !parent.parent.ok; text: qsTr("Ask on phone"); implicitHeight: 30
                          onClicked: Store.send("request_permissions", { permissions: [parent.parent.modelData] }) }
                }
            }
        }

        // Settings.
        Section {
            width: parent.width
            title: qsTr("On this computer")
            SettingRow {
                icon: "bell"
                title: qsTr("Show phone notifications on the desktop")
                detail: qsTr("They always appear in the Notifications tab.")
                Toggle { on: Store.settings.toasts; onToggled: on => Store.send("set", { key: "toasts", value: on }) }
            }
            SettingRow {
                icon: "phone"
                title: qsTr("Take calls on this computer")
                detail: Store.link.calls ? qsTr("Call audio is connected. Calls use this computer's speakers and microphone.")
                      : Store.connected && page.linked && !page.linked.calls ? qsTr("This phone doesn't offer hands-free calling over Bluetooth.")
                      : qsTr("Uses this computer's speakers and microphone over Bluetooth hands-free.")
                Toggle { on: Store.settings.calls; onToggled: on => Store.send("set", { key: "calls", value: on }) }
            }
            SettingRow {
                icon: "clipboard"
                title: qsTr("Send what I copy to my phone")
                detail: qsTr("Text only. Passwords from password managers are never sent.")
                Toggle { on: Store.settings.clipboard; onToggled: on => Store.send("set", { key: "clipboard", value: on }) }
            }
        }

        // Bluetooth.
        Section {
            width: parent.width
            title: qsTr("Bluetooth")
            detail: !Store.adapter ? qsTr("No Bluetooth adapter was found on this computer.")
                  : !page.bluetoothOn ? qsTr("Bluetooth is off.")
                  : Store.adapter.discovering ? qsTr("Scanning for nearby devices…") : qsTr("This computer is visible as “%1”.").arg(Store.adapter.name)
            trailing: Row {
                spacing: 10
                Btn {
                    visible: page.bluetoothOn
                    anchors.verticalCenter: parent.verticalCenter
                    text: Store.adapter && Store.adapter.discovering ? qsTr("Stop") : qsTr("Scan"); icon: "refresh"; iconSize: 14; implicitHeight: 30
                    onClicked: Store.send("scan", { on: !(Store.adapter && Store.adapter.discovering) })
                }
                Toggle { visible: !!Store.adapter; anchors.verticalCenter: parent.verticalCenter; on: page.bluetoothOn; onToggled: on => Store.send("power", { on: on }) }
            }

            Repeater {
                model: page.bluetoothOn ? (page.linked ? [page.linked] : []).concat(page.others) : []
                SettingRow {
                    id: device
                    required property var modelData
                    readonly property bool mine: modelData.address === Store.link.address
                    icon: modelData.phone ? "device" : "bluetooth"
                    iconColor: mine ? Theme.accent : Theme.secondary
                    title: modelData.name || modelData.address
                    detail: mine ? qsTr("Your linked phone") : modelData.paired ? qsTr("Paired") : modelData.phone ? qsTr("Phone nearby") : qsTr("Nearby")
                    Row {
                        spacing: 4
                        Btn { visible: !device.modelData.paired; text: qsTr("Pair"); kind: device.modelData.phone ? "primary" : "soft"; implicitHeight: 30
                              onClicked: Store.send("pair", { path: device.modelData.path }) }
                        Btn { visible: device.modelData.paired && !device.mine; text: qsTr("Use this phone"); implicitHeight: 30
                              onClicked: Store.send("use_device", { address: device.modelData.address }) }
                        Btn { visible: device.modelData.paired; icon: "trash"; iconSize: 15; kind: "ghost"; tint: Theme.secondary; implicitWidth: 30; implicitHeight: 30; hint: qsTr("Forget")
                              onClicked: forget.ask(device.modelData) }
                    }
                }
            }
            SettingRow {
                visible: page.bluetoothOn && Store.devices.length === 0
                title: qsTr("No devices yet")
                detail: qsTr("Press Scan with the phone's Bluetooth settings open.")
            }
        }

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Omalink %1 for Omarchy  ·  Press ? for keyboard shortcuts").arg(Qt.application.version)
            color: Theme.tertiary
            font.family: Theme.font; font.pixelSize: 12
        }
    }

    // Forgetting removes the pairing for the whole system, so ask first.
    property var forget: QtObject {
        property var target: null
        function ask(device) { target = device }
    }
    readonly property var forgetTarget: forget.target
    signal confirmForget(var device)
    onForgetTargetChanged: if (forgetTarget) confirmForget(forgetTarget)
}
