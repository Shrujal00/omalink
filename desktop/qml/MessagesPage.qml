import QtQuick
import QtQuick.Controls
import omalink

Item {
    id: page
    signal setup()

    property var current: null          // the open conversation
    property bool composing: false      // writing to someone new
    property string query: ""

    readonly property var filtered: {
        const q = query.trim().toLowerCase()
        if (!q) return Store.conversations
        return Store.conversations.filter(c => (c.name + " " + c.address + " " + c.snippet).toLowerCase().indexOf(q) >= 0)
    }

    function open(conversation) {
        composing = false
        current = conversation
        Store.thread = { thread_id: conversation.thread_id, items: [] }
        Store.pending = []
        Store.send("open_thread", { thread_id: conversation.thread_id })
        body.forceActiveFocus()
    }
    function compose(to) {
        current = null
        composing = true
        Store.thread = { thread_id: -2, items: [] }
        Store.send("close_thread")
        recipient.text = to || ""
        if (to) body.forceActiveFocus(); else recipient.input.forceActiveFocus()
    }
    function focusSearch() { search.input.forceActiveFocus() }
    function submit() {
        const to = composing ? recipient.text.trim() : (current ? current.address : "")
        const text = body.text.trim()
        if (!to || !text) return
        Store.send("send_sms", { to: to, body: text })
        Store.pending = Store.pending.concat([{ body: text, time: Date.now() }])
        body.text = ""
        messages.positionViewAtEnd()
    }

    // A message to someone new becomes a real conversation once the phone
    // reports it; follow it there.
    Connections {
        target: Store
        function onConversationsChanged() {
            if (page.composing && Store.pending.length === 0 && recipient.text) {
                const key = Store.numberKey(recipient.text)
                const match = Store.conversations.find(c => Store.numberKey(c.address) === key)
                if (match) page.open(match)
            } else if (page.current) {
                const fresh = Store.conversations.find(c => c.thread_id === page.current.thread_id)
                if (fresh) page.current = fresh
            }
        }
        function onThreadLoaded() { Qt.callLater(messages.positionViewAtEnd) }
    }

    Gate { id: gate; anchors.fill: parent; permission: "sms"; icon: "message"; what: qsTr("Messages"); onSetup: page.setup() }

    Item {
        anchors.fill: parent
        visible: !gate.blocked

        // Conversations.
        Item {
            id: list
            width: 312
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }

            Row {
                id: tools
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: 14 }
                spacing: 8
                Field { id: search; width: parent.width - 42; icon: "search"; placeholder: qsTr("Search"); onTextChanged: page.query = text }
                Btn { icon: "plus"; kind: "soft"; implicitWidth: 34; hint: qsTr("New message"); onClicked: page.compose("") }
            }

            ListView {
                id: threads
                anchors { left: parent.left; right: parent.right; top: tools.bottom; bottom: parent.bottom; topMargin: 10 }
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                // Every update from the phone is a whole new list, which
                // would throw the view back to the top; put it back where it
                // was unless the search is what changed.
                readonly property int rowHeight: 66
                property var rows: page.filtered
                property string shownQuery: ""
                function show() {
                    const y = page.query === shownQuery ? contentY : 0
                    shownQuery = page.query
                    model = rows
                    forceLayout()
                    contentY = Math.max(0, Math.min(y, count * rowHeight - height))
                }
                onRowsChanged: show()
                Component.onCompleted: show()

                delegate: Item {
                    id: row
                    required property var modelData
                    readonly property bool selected: page.current && page.current.thread_id === modelData.thread_id
                    readonly property string label: modelData.name || modelData.address
                    width: threads.width; height: threads.rowHeight

                    Rectangle {
                        anchors { fill: parent; leftMargin: 8; rightMargin: 8; topMargin: 1; bottomMargin: 1 }
                        radius: Theme.small
                        color: row.selected ? Theme.accentSoft : hover.containsMouse ? Theme.hover : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.quick } }
                    }
                    Avatar { id: face; name: row.label; anchors { left: parent.left; leftMargin: 18; verticalCenter: parent.verticalCenter } }
                    Text {
                        id: time
                        anchors { right: parent.right; rightMargin: 18; top: face.top; topMargin: 1 }
                        text: Store.when(row.modelData.time)
                        color: row.modelData.unread ? Theme.accent : Theme.tertiary
                        font.family: Theme.font; font.pixelSize: 11
                    }
                    Text {
                        anchors { left: face.right; leftMargin: 11; right: time.left; rightMargin: 8; top: face.top }
                        text: row.label
                        elide: Text.ElideRight
                        color: Theme.foreground
                        font.family: Theme.font; font.pixelSize: 14
                        font.weight: row.modelData.unread ? Font.Bold : Font.Medium
                    }
                    Text {
                        anchors { left: face.right; leftMargin: 11; right: dot.visible ? dot.left : parent.right; rightMargin: dot.visible ? 8 : 18; bottom: face.bottom }
                        text: row.modelData.snippet.replace(/\s+/g, " ")
                        elide: Text.ElideRight
                        color: row.modelData.unread ? Theme.foreground : Theme.secondary
                        font.family: Theme.font; font.pixelSize: 13
                    }
                    Rectangle {
                        id: dot
                        visible: row.modelData.unread > 0
                        width: 8; height: 8; radius: 4; color: Theme.accent
                        anchors { right: parent.right; rightMargin: 18; bottom: face.bottom; bottomMargin: 4 }
                    }
                    MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; onClicked: page.open(row.modelData) }
                }
                Empty {
                    anchors.centerIn: parent
                    visible: threads.count === 0
                    icon: page.query ? "search" : "message"
                    title: page.query ? qsTr("Nothing matches") : qsTr("No messages")
                    detail: page.query ? "" : qsTr("Texts on your phone will appear here.")
                }
            }
            Rectangle { width: 1; color: Theme.line; anchors { right: parent.right; top: parent.top; bottom: parent.bottom } }
        }

        // The open conversation.
        Item {
            anchors { left: list.right; right: parent.right; top: parent.top; bottom: parent.bottom }

            Empty {
                anchors.centerIn: parent
                visible: !page.current && !page.composing
                icon: "message"
                title: qsTr("Pick a conversation")
                detail: qsTr("Or start a new one with Ctrl+N.")
            }

            Item {
                anchors.fill: parent
                visible: page.current || page.composing

                Item {
                    id: header
                    height: 62
                    anchors { left: parent.left; right: parent.right; top: parent.top }
                    Row {
                        visible: !page.composing
                        anchors { left: parent.left; leftMargin: 20; verticalCenter: parent.verticalCenter }
                        spacing: 11
                        Avatar { name: page.current ? (page.current.name || page.current.address) : ""; size: 34 }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            Text { text: page.current ? (page.current.name || page.current.address) : ""; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.DemiBold }
                            Text { visible: page.current && page.current.name; text: page.current ? page.current.address : ""; color: Theme.tertiary; font.family: Theme.font; font.pixelSize: 12 }
                        }
                    }
                    Row {
                        visible: page.composing
                        anchors { left: parent.left; leftMargin: 20; right: parent.right; rightMargin: 20; verticalCenter: parent.verticalCenter }
                        spacing: 10
                        Text { text: qsTr("To"); color: Theme.tertiary; font.family: Theme.font; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
                        Field { id: recipient; width: parent.width - 40; placeholder: qsTr("Name or number"); onAccepted: body.forceActiveFocus() }
                    }
                    Btn {
                        visible: !page.composing && page.current && /\d{3}/.test(page.current.address)
                        anchors { right: parent.right; rightMargin: 16; verticalCenter: parent.verticalCenter }
                        icon: "phone"; kind: "ghost"; implicitWidth: 34; hint: qsTr("Call")
                        onClicked: Store.send("dial", { number: page.current.address })
                    }
                    Rectangle { height: 1; color: Theme.line; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } }
                }

                // People matching what is typed in "To".
                ListView {
                    id: suggestions
                    z: 2
                    readonly property var matches: {
                        const q = recipient.text.trim().toLowerCase()
                        if (!page.composing || q.length < 2) return []
                        const out = []
                        for (const c of Store.contacts) {
                            for (const n of c.numbers) {
                                if (c.name.toLowerCase().indexOf(q) >= 0 || n.replace(/\s/g, "").indexOf(q) >= 0)
                                    out.push({ name: c.name, number: n })
                                if (out.length >= 5) return out
                            }
                        }
                        return out.length === 1 && out[0].number === recipient.text ? [] : out
                    }
                    visible: matches.length > 0 && recipient.input.activeFocus
                    anchors { left: parent.left; right: parent.right; top: header.bottom; leftMargin: 54; rightMargin: 20 }
                    height: contentHeight
                    model: matches
                    interactive: false
                    delegate: Rectangle {
                        required property var modelData
                        width: suggestions.width; height: 44
                        color: pick.containsMouse ? Theme.raised : Theme.surface
                        Text { anchors { left: parent.left; leftMargin: 12; verticalCenter: parent.verticalCenter }
                               text: modelData.name; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 13 }
                        Text { anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                               text: modelData.number; color: Theme.tertiary; font.family: Theme.font; font.pixelSize: 12 }
                        // Pressed, not clicked: the field loses focus first otherwise and the list is gone.
                        MouseArea { id: pick; anchors.fill: parent; hoverEnabled: true; onPressed: { recipient.text = modelData.number; body.forceActiveFocus() } }
                    }
                }

                ListView {
                    id: messages
                    anchors { left: parent.left; right: parent.right; top: header.bottom; bottom: composer.top }
                    clip: true
                    topMargin: 14; bottomMargin: 10
                    spacing: 3
                    boundsBehavior: Flickable.StopAtBounds
                    verticalLayoutDirection: ListView.TopToBottom
                    model: Store.thread.items.concat(Store.pending.map(p => ({ id: -1, body: p.body, time: p.time, outgoing: true, pending: true })))
                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                    onCountChanged: Qt.callLater(positionViewAtEnd)
                    delegate: Item {
                        id: line
                        required property var modelData
                        required property int index
                        readonly property var previous: index > 0 ? messages.model[index - 1] : null
                        // A day label when the date changes, a gap when the speaker does.
                        readonly property bool newDay: !previous || new Date(previous.time).toDateString() !== new Date(modelData.time).toDateString()
                        readonly property bool newSpeaker: !previous || previous.outgoing !== modelData.outgoing
                        width: messages.width
                        height: bubble.height + (newDay ? 40 : newSpeaker ? 9 : 0)

                        Text {
                            visible: line.newDay
                            anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: 12 }
                            text: Store.day(line.modelData.time)
                            color: Theme.tertiary
                            font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Medium
                        }
                        Rectangle {
                            id: bubble
                            anchors.bottom: parent.bottom
                            x: line.modelData.outgoing ? parent.width - width - 20 : 20
                            width: Math.min(words.implicitWidth + 24, Math.max(160, messages.width * 0.66))
                            height: words.height + 16
                            radius: Math.max(Theme.small, Math.min(14, Theme.radius))
                            color: line.modelData.outgoing ? Theme.accent : Theme.surface
                            opacity: line.modelData.pending ? 0.55 : 1
                            Text {
                                id: words
                                x: 12; y: 8
                                width: Math.min(implicitWidth, Math.max(136, messages.width * 0.66 - 24))
                                text: line.modelData.body
                                wrapMode: Text.Wrap
                                textFormat: Text.PlainText
                                color: line.modelData.outgoing ? Theme.onAccent : Theme.foreground
                                font.family: Theme.font; font.pixelSize: 14
                                lineHeight: 1.2
                            }
                            HoverHandler { id: over }
                        }
                        Text {
                            visible: over.hovered || line.modelData.pending === true
                            anchors.verticalCenter: bubble.verticalCenter
                            x: line.modelData.outgoing ? bubble.x - width - 8 : bubble.x + bubble.width + 8
                            text: line.modelData.pending ? qsTr("Sending…") : Store.clock(line.modelData.time)
                            color: Theme.tertiary
                            font.family: Theme.font; font.pixelSize: 11
                        }
                    }
                }

                Item {
                    id: composer
                    height: Math.min(140, Math.max(40, body.implicitHeight + 6)) + 24
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                    Rectangle {
                        anchors { fill: parent; leftMargin: 16; rightMargin: 62; topMargin: 10; bottomMargin: 14 }
                        radius: Theme.small
                        color: Theme.hover
                        border.width: 1
                        border.color: body.activeFocus ? Theme.accent : "transparent"
                        Flickable {
                            anchors { fill: parent; leftMargin: 12; rightMargin: 8; topMargin: 2; bottomMargin: 2 }
                            contentHeight: body.implicitHeight
                            clip: true
                            TextArea.flickable: TextArea {
                                id: body
                                placeholderText: qsTr("Text message")
                                placeholderTextColor: Theme.tertiary
                                color: Theme.foreground
                                selectionColor: Theme.accentSoft
                                selectedTextColor: Theme.foreground
                                font.family: Theme.font; font.pixelSize: 14
                                wrapMode: TextEdit.Wrap
                                background: null
                                leftPadding: 0; rightPadding: 0; topPadding: 9; bottomPadding: 9
                                // Enter sends; Shift+Enter makes a new line.
                                Keys.onPressed: event => {
                                    if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
                                        page.submit()
                                        event.accepted = true
                                    }
                                }
                            }
                        }
                    }
                    Btn {
                        anchors { right: parent.right; rightMargin: 16; bottom: parent.bottom; bottomMargin: 15 }
                        icon: "send"; kind: "primary"; implicitWidth: 38; implicitHeight: 38; hint: qsTr("Send")
                        enabled: body.text.trim() !== "" && (!page.composing || recipient.text.trim() !== "")
                        onClicked: page.submit()
                    }
                }
            }
        }
    }
}
