import QtQuick
import omalink

// A small status tag: "From phone", "Relay", "Wi-Fi direct".
Rectangle {
    id: chip
    property string text: ""
    property string icon: ""
    property color tint: Theme.secondary
    property bool strong: false

    implicitWidth: row.implicitWidth + 16
    implicitHeight: 22
    radius: Theme.radius > 0 ? height / 2 : 2
    color: Qt.rgba(tint.r, tint.g, tint.b, strong ? 0.22 : 0.12)

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 5
        Icon { visible: chip.icon !== ""; name: chip.icon; size: 12; weight: 1.8; color: chip.tint; anchors.verticalCenter: parent.verticalCenter }
        Text {
            text: chip.text
            color: chip.tint
            font.family: Theme.font; font.pixelSize: 11; font.weight: Font.DemiBold
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
