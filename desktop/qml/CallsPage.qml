import QtQuick
import QtQuick.Controls
import omalink

Item {
    id: page
    signal setup()
    signal message(string number)

    property bool missedOnly: false
    property string number: ""

    readonly property var history: missedOnly ? Store.callLog.filter(c => c.kind === "missed" || c.kind === "rejected") : Store.callLog
    readonly property var matches: {
        const q = number.replace(/\s/g, "").toLowerCase()
        if (q.length < 2) return []
        const out = []
        for (const c of Store.contacts)
            for (const n of c.numbers) {
                if (n.replace(/[\s()-]/g, "").indexOf(q) >= 0 || c.name.toLowerCase().indexOf(q) >= 0)
                    out.push({ name: c.name, number: n })
                if (out.length >= 3) return out
            }
        return out
    }

    function dial(n) {
        const target = (n || number).trim()
        if (target) Store.send("dial", { number: target })
    }
    function focusDial() { entry.forceActiveFocus() }

    Gate { id: gate; anchors.fill: parent; icon: "phone"; what: qsTr("Calls"); onSetup: page.setup() }

    Item {
        anchors.fill: parent
        visible: !gate.blocked

        // Recent calls.
        Item {
            id: recents
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom; right: pad.left }

            Row {
                id: filter
                anchors { left: parent.left; top: parent.top; margins: 14; leftMargin: 20 }
                spacing: 6
                Repeater {
                    model: [qsTr("All"), qsTr("Missed")]
                    Btn {
                        required property string modelData
                        required property int index
                        text: modelData
                        kind: (index === 1) === page.missedOnly ? "soft" : "ghost"
                        tint: (index === 1) === page.missedOnly ? Theme.foreground : Theme.secondary
                        implicitHeight: 30
                        onClicked: page.missedOnly = index === 1
                    }
                }
            }

            ListView {
                id: log
                anchors { left: parent.left; right: parent.right; top: filter.bottom; bottom: parent.bottom; topMargin: 8 }
                clip: true
                model: page.history
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                delegate: Item {
                    id: row
                    required property var modelData
                    readonly property bool bad: modelData.kind === "missed" || modelData.kind === "rejected"
                    readonly property string label: modelData.name || modelData.number || qsTr("Unknown")
                    width: log.width; height: 60

                    Rectangle {
                        anchors { fill: parent; leftMargin: 8; rightMargin: 8; topMargin: 1; bottomMargin: 1 }
                        radius: Theme.small
                        color: over.hovered ? Theme.hover : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.quick } }
                    }
                    HoverHandler { id: over }
                    Avatar { id: face; name: row.label; size: 36; anchors { left: parent.left; leftMargin: 20; verticalCenter: parent.verticalCenter } }
                    Text {
                        anchors { left: face.right; leftMargin: 12; right: actions.left; rightMargin: 10; top: face.top; topMargin: 1 }
                        text: row.label
                        elide: Text.ElideRight
                        color: row.bad ? Theme.red : Theme.foreground
                        font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium
                    }
                    Row {
                        anchors { left: face.right; leftMargin: 11; bottom: face.bottom }
                        spacing: 5
                        Icon { name: row.modelData.kind === "outgoing" ? "out" : "in"; size: 12; weight: 1.8
                               color: row.bad ? Theme.red : Theme.tertiary; anchors.verticalCenter: parent.verticalCenter }
                        Text {
                            text: {
                                const kind = { incoming: qsTr("Incoming"), outgoing: qsTr("Outgoing"), missed: qsTr("Missed"), rejected: qsTr("Declined") }[row.modelData.kind]
                                return row.modelData.duration > 0 ? kind + " · " + Store.duration(row.modelData.duration) : kind
                            }
                            color: Theme.secondary
                            font.family: Theme.font; font.pixelSize: 12
                        }
                    }
                    Row {
                        id: actions
                        anchors { right: parent.right; rightMargin: 18; verticalCenter: parent.verticalCenter }
                        spacing: 2
                        Text {
                            visible: !over.hovered
                            text: Store.when(row.modelData.time)
                            color: Theme.tertiary
                            font.family: Theme.font; font.pixelSize: 12
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Btn { visible: over.hovered && row.modelData.number; icon: "message"; kind: "ghost"; implicitWidth: 32; implicitHeight: 32; hint: qsTr("Message"); onClicked: page.message(row.modelData.number) }
                        Btn { visible: over.hovered && row.modelData.number; icon: "phone"; kind: "ghost"; implicitWidth: 32; implicitHeight: 32; hint: qsTr("Call"); tint: Theme.green; onClicked: page.dial(row.modelData.number) }
                    }
                    TapHandler { onDoubleTapped: page.dial(row.modelData.number) }
                }
                Empty {
                    anchors.centerIn: parent
                    visible: log.count === 0
                    icon: "phone"
                    title: !Store.has("call_log") ? qsTr("Call history needs permission") : page.missedOnly ? qsTr("No missed calls") : qsTr("No recent calls")
                    detail: !Store.has("call_log") ? qsTr("Allow it on your phone to see recent calls.") : ""
                    action: !Store.has("call_log") ? qsTr("Ask on phone") : ""
                    onTriggered: Store.send("request_permissions", { permissions: ["call_log", "contacts"] })
                }
            }
        }

        // The dial pad.
        Item {
            id: pad
            width: 300
            anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
            Rectangle { width: 1; color: Theme.line; anchors { left: parent.left; top: parent.top; bottom: parent.bottom } }

            Column {
                anchors { horizontalCenter: parent.horizontalCenter; verticalCenter: parent.verticalCenter; verticalCenterOffset: -6 }
                width: 228
                spacing: 14

                Item {
                    width: parent.width; height: 44
                    TextInput {
                        id: entry
                        anchors { left: parent.left; right: erase.left; rightMargin: 4; verticalCenter: parent.verticalCenter }
                        horizontalAlignment: TextInput.AlignHCenter
                        text: page.number
                        onTextEdited: page.number = text
                        color: Theme.foreground
                        selectionColor: Theme.accentSoft
                        selectedTextColor: Theme.foreground
                        font.family: Theme.font
                        font.pixelSize: text.length > 14 ? 18 : 24
                        font.weight: Font.Medium
                        clip: true
                        selectByMouse: true
                        onAccepted: page.dial()
                        Text {
                            visible: !entry.text
                            anchors.centerIn: parent
                            text: qsTr("Name or number")
                            color: Theme.tertiary
                            font.family: Theme.font; font.pixelSize: 15
                        }
                    }
                    Btn {
                        id: erase
                        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                        opacity: page.number ? 1 : 0
                        icon: "backspace"; kind: "ghost"; implicitWidth: 32; implicitHeight: 32; tint: Theme.secondary; hint: qsTr("Delete")
                        onClicked: page.number = page.number.slice(0, -1)
                        Behavior on opacity { NumberAnimation { duration: Theme.quick } }
                    }
                }

                // Contacts matching what is typed; the pad gives way to them.
                Column {
                    width: parent.width
                    visible: page.matches.length > 0
                    spacing: 2
                    Repeater {
                        model: page.matches
                        Rectangle {
                            required property var modelData
                            width: parent.width; height: 42; radius: Theme.small
                            color: pick.containsMouse ? Theme.hover : "transparent"
                            Text { anchors { left: parent.left; leftMargin: 10; top: parent.top; topMargin: 5 }
                                   text: modelData.name; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 13 }
                            Text { anchors { left: parent.left; leftMargin: 10; bottom: parent.bottom; bottomMargin: 5 }
                                   text: modelData.number; color: Theme.tertiary; font.family: Theme.font; font.pixelSize: 12 }
                            MouseArea { id: pick; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: page.number = modelData.number }
                        }
                    }
                }

                Grid {
                    columns: 3
                    spacing: 12
                    visible: page.matches.length === 0 || /^[\d\s+*#()-]*$/.test(page.number)
                    Repeater {
                        model: [["1", ""], ["2", "ABC"], ["3", "DEF"], ["4", "GHI"], ["5", "JKL"], ["6", "MNO"], ["7", "PQRS"], ["8", "TUV"], ["9", "WXYZ"], ["*", ""], ["0", "+"], ["#", ""]]
                        Rectangle {
                            id: key
                            required property var modelData
                            width: 68; height: 54
                            radius: Theme.radius > 0 ? Math.min(27, Theme.radius * 2) : 0
                            color: press.pressed ? Theme.pressed : press.containsMouse ? Theme.hover : Theme.surface
                            scale: press.pressed ? 0.95 : 1
                            Behavior on color { ColorAnimation { duration: 90 } }
                            Behavior on scale { NumberAnimation { duration: 90 } }
                            Column {
                                anchors.centerIn: parent
                                Text { text: key.modelData[0]; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 19; font.weight: Font.Medium; anchors.horizontalCenter: parent.horizontalCenter }
                                Text { visible: text; text: key.modelData[1]; color: Theme.tertiary; font.family: Theme.font; font.pixelSize: 9; font.letterSpacing: 1.2; anchors.horizontalCenter: parent.horizontalCenter }
                            }
                            MouseArea {
                                id: press
                                anchors.fill: parent; hoverEnabled: true
                                onClicked: page.number += key.modelData[0]
                                // Holding 0 types +, as on a phone.
                                onPressAndHold: if (key.modelData[0] === "0") page.number += "+"
                            }
                        }
                    }
                }

                Rectangle {
                    width: 68; height: 54
                    anchors.horizontalCenter: parent.horizontalCenter
                    radius: Theme.radius > 0 ? 27 : 0
                    color: go.pressed ? Qt.darker(Theme.green, 1.15) : Theme.green
                    opacity: page.number.trim() ? 1 : 0.35
                    scale: go.pressed ? 0.95 : 1
                    Behavior on opacity { NumberAnimation { duration: Theme.quick } }
                    Behavior on scale { NumberAnimation { duration: 90 } }
                    Icon { anchors.centerIn: parent; name: "phone"; size: 22; color: "#10140f"; weight: 1.8 }
                    MouseArea { id: go; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: page.dial() }
                }

                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    visible: !Store.link.calls
                    text: qsTr("Call audio isn't connected, so calls ring and talk on the phone.")
                    color: Theme.tertiary
                    font.family: Theme.font; font.pixelSize: 12
                }
                Btn {
                    visible: !Store.link.calls
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("Connect call audio"); kind: "ghost"; tint: Theme.accent; implicitHeight: 28
                    onClicked: Store.send("connect_calls")
                }
            }
        }
    }
}
