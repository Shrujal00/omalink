import QtQuick
import omalink

// Stands in for a page that can't show anything yet: no phone, or a missing
// permission. It always says which, and offers the way forward.
Item {
    id: gate
    property string permission: ""
    property string icon: "device"
    property string what: ""
    signal setup()

    readonly property bool blocked: !Store.connected || (permission !== "" && !Store.has(permission))

    visible: blocked
    Empty {
        anchors.centerIn: parent
        icon: gate.icon
        title: !Store.connected
               ? (Store.link.state === "none" ? qsTr("No phone linked yet") : qsTr("Phone not connected"))
               : qsTr("%1 needs permission").arg(gate.what)
        detail: !Store.connected
                ? (Store.link.state === "none"
                   ? qsTr("Pair your Android phone to see its %1 here.").arg(gate.what.toLowerCase())
                   : (Store.link.error || qsTr("Looking for your phone…")))
                : qsTr("Allow it on your phone and this fills in right away.")
        action: !Store.connected ? qsTr("Set up phone") : qsTr("Ask on phone")
        onTriggered: Store.connected ? Store.send("request_permissions", { permissions: [gate.permission] }) : gate.setup()
    }
}
