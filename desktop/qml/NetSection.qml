import QtQuick
import omalink

// The phone over the network: pairing with icons, the phones that are
// paired, and where traffic goes when no direct path works.
Section {
    id: section
    readonly property var net: Store.net
    readonly property var pairing: net.pairing
    readonly property string routeText: net.route === "direct" ? qsTr("direct") : net.route === "relay" ? qsTr("through a relay") : qsTr("finding a path…")

    title: qsTr("Phone over the network")
    detail: net.state === "starting" ? qsTr("Starting…")
          : net.state === "failed" ? net.error
          : net.state === "off" ? qsTr("Off")
          : net.connected ? qsTr("Connected, %1. Files and screen sharing work on any network.").arg(routeText)
          : net.phones.length ? qsTr("Your phone connects here by itself whenever it has internet, on any network.")
          : qsTr("Pair once and your phone reaches this computer from anywhere: the same Wi-Fi, another network, or mobile data.")
    trailing: Btn {
        visible: section.net.state === "ready"
        text: section.pairing ? qsTr("Cancel") : qsTr("Pair a phone")
        icon: section.pairing ? "" : "wifi"; iconSize: 15
        kind: section.pairing ? "ghost" : "primary"; implicitHeight: 30
        onClicked: Store.send("net_pair", { on: !section.pairing })
    }

    // Pairing: first where to look on the phone, then the icon to match.
    Item {
        visible: !!section.pairing
        width: parent.width
        height: visible ? pairBody.height + 36 : 0
        Column {
            id: pairBody
            width: parent.width - 32
            anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: 18 }
            spacing: 10

            Rectangle {
                visible: !!section.pairing && section.pairing.icon !== ""
                anchors.horizontalCenter: parent.horizontalCenter
                width: 132; height: 132; radius: Theme.radius > 0 ? 66 : 0
                color: Theme.accentSoft
                Text {
                    anchors.centerIn: parent
                    text: section.pairing ? section.pairing.icon : ""
                    font.pixelSize: 72
                    textFormat: Text.PlainText
                    Accessible.name: section.pairing ? section.pairing.icon_name : ""
                }
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: !section.pairing ? ""
                    : section.pairing.icon !== "" ? qsTr("On %1, tap the %2.").arg(section.pairing.phone || qsTr("your phone")).arg(section.pairing.icon_name)
                    : section.pairing.phone !== "" ? qsTr("%1 is connecting…").arg(section.pairing.phone)
                    : qsTr("On your phone, open Omalink, tap Pair with a computer, and choose “%1”.").arg(section.net.name)
                color: Theme.foreground
                font.family: Theme.font; font.pixelSize: 15; font.weight: Font.Medium
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                text: !section.pairing ? ""
                    : section.pairing.icon !== "" ? qsTr("The phone shows three icons. Only the one here pairs; a wrong tap means someone else may be in between.")
                    : qsTr("On the same Wi-Fi it appears by itself. Otherwise, connect the phone over Bluetooth first and it learns where to find this computer.")
                color: Theme.secondary
                font.family: Theme.font; font.pixelSize: 12
            }
            Text {
                visible: !!section.pairing && section.pairing.attempts > 0
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: section.pairing ? qsTr("%n wrong tap(s). %1 left.", "", section.pairing.attempts).arg(section.pairing.left) : ""
                color: Theme.yellow
                font.family: Theme.font; font.pixelSize: 12
            }
        }
    }

    Repeater {
        model: section.net.phones
        SettingRow {
            id: phone
            required property var modelData
            icon: "device"
            iconColor: modelData.connected ? Theme.accent : Theme.secondary
            title: modelData.name
            detail: modelData.connected ? qsTr("Connected, %1").arg(section.routeText) : qsTr("Paired %1").arg(Store.when(modelData.added))
            Btn { icon: "trash"; iconSize: 15; kind: "ghost"; tint: Theme.secondary; implicitWidth: 30; implicitHeight: 30; hint: qsTr("Forget")
                  onClicked: Store.send("net_forget", { id: phone.modelData.id }) }
        }
    }

    SettingRow {
        icon: "globe"
        title: qsTr("Relay server")
        detail: qsTr("Only used when no direct path works; it carries encrypted bytes it can't read. Leave empty for the public relays, or enter your own iroh-relay.")
        Row {
            spacing: 6
            Field {
                id: relay
                width: 190
                placeholder: qsTr("Public relays")
                text: section.net.relay
                onAccepted: Store.send("net_relay", { url: text })
            }
            Btn { visible: relay.text !== section.net.relay; text: qsTr("Use"); implicitHeight: 34
                  onClicked: Store.send("net_relay", { url: relay.text }) }
        }
    }
}
