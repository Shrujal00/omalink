import QtQuick
import QtQuick.Controls
import omalink

Item {
    id: page
    signal setup()
    signal message(string number)

    property bool missedOnly: false
    property string number: ""
    property string query: ""

    function hit(name, number) {
        const q = page.query.trim().toLowerCase()
        if (!q) return true
        const digits = q.replace(/[\s()+-]/g, "")
        return (name || "").toLowerCase().indexOf(q) >= 0
            || (digits.length > 0 && /^\d+$/.test(digits) && (number || "").replace(/[\s()+-]/g, "").indexOf(digits) >= 0)
    }
    readonly property var history: (missedOnly ? Store.callLog.filter(c => c.kind === "missed" || c.kind === "rejected") : Store.callLog)
                                    .filter(c => page.hit(c.name, c.number))
    // Everyone in the phone's contacts who matches the search.
    readonly property var people: {
        if (!query.trim()) return []
        const out = []
        for (const c of Store.contacts) {
            if (c.numbers.length && (page.hit(c.name, "") || c.numbers.some(n => page.hit("", n)))) out.push(c)
            if (out.length >= 40) break
        }
        return out
    }
    function focusSearch() { search.forceActiveFocus() }
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

            // Search everyone, and whether to see only missed calls.
            Item {
                id: filter
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.s4; leftMargin: Theme.s5; rightMargin: Theme.s5 }
                height: 44
                Rectangle {
                    anchors { left: parent.left; right: toggles.left; rightMargin: Theme.s2; top: parent.top; bottom: parent.bottom }
                    radius: Theme.card
                    color: Theme.surface
                    border.width: 1; border.color: search.activeFocus ? Theme.accentLine : Theme.line
                    Icon { id: glass; name: "search"; size: 17; color: Theme.tertiary; anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter } }
                    TextInput {
                        id: search
                        anchors { left: glass.right; leftMargin: 10; right: clear.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
                        color: Theme.foreground
                        font.family: Theme.font; font.pixelSize: Theme.body
                        clip: true; selectByMouse: true
                        onTextEdited: page.query = text
                        Keys.onEscapePressed: { text = ""; page.query = "" }
                        Text { visible: !search.text; text: qsTr("Search contacts"); color: Theme.tertiary; font: search.font; anchors.verticalCenter: parent.verticalCenter }
                    }
                    Btn { id: clear; visible: page.query !== ""; icon: "x"; iconSize: 14; kind: "ghost"; tint: Theme.tertiary; implicitWidth: 30; implicitHeight: 30
                          anchors { right: parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
                          onClicked: { search.text = ""; page.query = "" } }
                }
                Row {
                    id: toggles
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    spacing: 4
                    Btn { icon: "phone"; iconSize: 16; implicitWidth: 40; implicitHeight: 40; hint: qsTr("All calls")
                          kind: !page.missedOnly ? "soft" : "ghost"; tint: !page.missedOnly ? Theme.accent : Theme.secondary
                          onClicked: page.missedOnly = false }
                    Btn { icon: "in"; iconSize: 16; implicitWidth: 40; implicitHeight: 40; hint: qsTr("Missed calls")
                          kind: page.missedOnly ? "soft" : "ghost"; tint: page.missedOnly ? Theme.red : Theme.secondary
                          onClicked: page.missedOnly = true }
                }
            }

            ListView {
                id: log
                anchors { left: parent.left; right: parent.right; top: filter.bottom; bottom: parent.bottom; topMargin: 8 }
                clip: true
                model: page.history
                boundsBehavior: Flickable.StopAtBounds
                header: Column {
                    width: log.width
                    visible: page.people.length > 0
                    height: visible ? implicitHeight + Theme.s3 : 0
                    Repeater {
                        model: page.people
                        Item {
                            id: person
                            required property var modelData
                            width: log.width; height: 60
                            Rectangle {
                                anchors { fill: parent; leftMargin: 8; rightMargin: 8; topMargin: 1; bottomMargin: 1 }
                                radius: Theme.small
                                color: personHover.hovered ? Theme.hover : "transparent"
                            }
                            HoverHandler { id: personHover }
                            Avatar { id: personFace; name: person.modelData.name; size: 36; anchors { left: parent.left; leftMargin: 20; verticalCenter: parent.verticalCenter } }
                            Column {
                                anchors { left: personFace.right; leftMargin: 12; right: personActions.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
                                spacing: 2
                                Text { width: parent.width; elide: Text.ElideRight; text: person.modelData.name; color: Theme.foreground
                                       font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium; textFormat: Text.PlainText }
                                Text { width: parent.width; elide: Text.ElideRight; text: person.modelData.numbers.join("  ·  "); color: Theme.tertiary
                                       font.family: Theme.font; font.pixelSize: 12 }
                            }
                            Row {
                                id: personActions
                                anchors { right: parent.right; rightMargin: 18; verticalCenter: parent.verticalCenter }
                                spacing: 2
                                Btn { icon: "message"; kind: "ghost"; implicitWidth: 34; implicitHeight: 34; hint: qsTr("Message"); onClicked: page.message(person.modelData.numbers[0]) }
                                Btn { icon: "phone"; kind: "ghost"; implicitWidth: 34; implicitHeight: 34; hint: qsTr("Call %1").arg(person.modelData.numbers[0]); tint: Theme.green
                                      onClicked: page.dial(person.modelData.numbers[0]) }
                            }
                            TapHandler { onDoubleTapped: page.dial(person.modelData.numbers[0]) }
                        }
                    }
                    Rectangle { visible: log.count > 0; width: log.width - 56; height: 1; color: Theme.line; x: 28 }
                }
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
                            // The arrow says which way; only how long is left to say.
                            text: row.modelData.duration > 0 ? Store.duration(row.modelData.duration)
                                : row.bad ? (row.modelData.kind === "missed" ? qsTr("Missed") : qsTr("Declined")) : ""
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
                    icon: "phone"
                    visible: log.count === 0 && page.people.length === 0
                    title: !Store.has("call_log") ? qsTr("Call history needs permission") : page.query ? qsTr("No one by that name") : page.missedOnly ? qsTr("No missed calls") : qsTr("No recent calls")
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

                Btn {
                    visible: !Store.link.calls
                    anchors.horizontalCenter: parent.horizontalCenter
                    icon: "mic"; iconSize: 17; kind: "soft"; tint: Theme.accent; implicitWidth: 40; implicitHeight: 36
                    hint: qsTr("Connect call audio  ·  until then calls ring and talk on the phone")
                    onClicked: Store.send("connect_calls")
                }
            }
        }
    }
}
