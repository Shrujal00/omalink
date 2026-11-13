pragma Singleton
import QtQuick

// What the core last said about the world. Nothing here is decided by the
// window; it only remembers and formats.
QtObject {
    id: store

    property var backend: null

    property var adapter: null
    property var devices: []
    property var link: ({ state: "none", address: "", name: "", error: "", calls: false })
    property var settings: ({ device: "", clipboard: true, toasts: true, calls: true })
    property var status: null
    property var granted: []
    property var missing: []
    property var notifications: []
    property var conversations: []
    property var thread: ({ thread_id: -1, items: [] })
    property var pending: []
    property var callLog: []
    property var contacts: []
    // Contact picture by lowercased name and by number key.
    property var photos: ({})
    // The phone's wallpaper as a file URL, or empty. A new wallpaper is a new file.
    property string wallpaper: ""
    property var call: ({ phase: "idle", number: "", name: "", since: 0, audio: false, muted: false })
    property var clips: []
    property var transfers: []
    property var pairing: null

    readonly property bool connected: link.state === "connected"
    readonly property int unread: conversations.reduce((n, c) => n + (c.unread || 0), 0)
    readonly property int missed: 0

    signal note(string text)
    signal show(string page)
    signal ready(bool visible)
    signal threadLoaded()
    signal quit()
    signal shot(string path, int page, int delay)

    // Ticks once a minute so "4 min ago" stays true without anything changing.
    property int minute: 0
    property Timer clockTimer: Timer {
        interval: 30000; running: true; repeat: true
        onTriggered: store.minute++
    }

    function send(command, args) {
        if (backend)
            backend.send(command, JSON.stringify(args || {}))
    }

    function apply(topic, data) {
        switch (topic) {
        case "theme": Theme.apply(data); break
        case "adapter": adapter = data; break
        case "devices": devices = data || []; break
        case "link": link = data; break
        case "settings": settings = data; break
        case "status": status = data; break
        case "permissions": granted = data.granted; missing = data.missing; break
        case "notifications": notifications = data || []; break
        case "conversations": conversations = data || []; break
        case "messages":
            if (data.thread_id === thread.thread_id || thread.thread_id === -1) {
                thread = data
                threadLoaded()
            }
            break
        case "sms_sent": pending = pending.slice(1); break
        case "call_log": callLog = data || []; break
        case "contacts":
            contacts = data || []
            const found = {}
            for (const c of contacts) {
                if (!c.photo) continue
                found[c.name.toLowerCase()] = c.photo
                for (const n of c.numbers) found[numberKey(n)] = c.photo
            }
            photos = found
            break
        case "wallpaper": wallpaper = data || ""; break
        case "call": call = data; break
        case "clips": clips = data || []; break
        case "transfers": transfers = data || []; break
        case "pairing": pairing = data; break
        case "note": note(data.text); break
        case "show": show(data && data.page ? data.page : ""); break
        case "ready": ready(data.show); break
        case "shot": shot(data.path, data.page, data.delay || 900); break
        case "quit": quit(); break
        }
    }

    readonly property var permissionInfo: ({
        "bluetooth": [qsTr("Nearby devices"), qsTr("Stay connected to this computer")],
        "post_notifications": [qsTr("Show notifications"), qsTr("Keep the connection alive in the background")],
        "notifications": [qsTr("Notification access"), qsTr("Show your phone's notifications here")],
        "sms": [qsTr("Messages"), qsTr("Read and send text messages")],
        "contacts": [qsTr("Contacts"), qsTr("Show names instead of numbers")],
        "call_log": [qsTr("Call history"), qsTr("Show recent calls")],
        "phone": [qsTr("Phone"), qsTr("Place, answer, and end calls")],
        "battery": [qsTr("Run in background"), qsTr("Stop Android from putting the app to sleep")],
        "wallpaper": [qsTr("Wallpaper"), qsTr("Show the phone's wallpaper here. Android asks for all files access to read it")]
    })

    function has(permission) { return granted.indexOf(permission) >= 0 }

    function numberKey(number) {
        const digits = String(number).replace(/\D/g, "")
        if (digits.length > 10) return digits.slice(-10)
        return digits.length ? digits : String(number).trim().toLowerCase()
    }

    function nameFor(number) {
        const key = numberKey(number)
        for (const c of contacts)
            for (const n of c.numbers)
                if (numberKey(n) === key) return c.name
        return ""
    }

    function photoFor(label) {
        const text = String(label)
        return photos[text.toLowerCase()] || photos[numberKey(text)] || ""
    }

    function initials(name) {
        const parts = String(name).trim().split(/\s+/).filter(p => /^[^\d+()#*\-.]/.test(p))
        if (!parts.length) return "#"
        return (parts[0][0] + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase()
    }

    // Short and relative when recent, a date when not.
    function when(ms) {
        minute
        const now = Date.now(), d = new Date(ms), minutes = Math.floor((now - ms) / 60000)
        if (minutes < 1) return qsTr("now")
        if (minutes < 60) return qsTr("%1 min").arg(minutes)
        const today = new Date(); today.setHours(0, 0, 0, 0)
        if (ms >= today.getTime()) return d.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)
        if (ms >= today.getTime() - 86400000) return qsTr("Yesterday")
        if (ms >= today.getTime() - 6 * 86400000) return d.toLocaleDateString(Qt.locale(), "ddd")
        return d.toLocaleDateString(Qt.locale(), "d MMM")
    }

    function clock(ms) { return new Date(ms).toLocaleTimeString(Qt.locale(), Locale.ShortFormat) }

    function day(ms) {
        const today = new Date(); today.setHours(0, 0, 0, 0)
        if (ms >= today.getTime()) return qsTr("Today")
        if (ms >= today.getTime() - 86400000) return qsTr("Yesterday")
        return new Date(ms).toLocaleDateString(Qt.locale(), "dddd, d MMMM")
    }

    function duration(seconds) {
        const m = Math.floor(seconds / 60), s = Math.floor(seconds % 60)
        if (m >= 60) return qsTr("%1 h %2 min").arg(Math.floor(m / 60)).arg(m % 60)
        if (m > 0) return qsTr("%1 min %2 s").arg(m).arg(s)
        return qsTr("%1 s").arg(s)
    }

    function timer(ms) {
        const t = Math.max(0, Math.floor(ms / 1000)), m = Math.floor(t / 60), s = t % 60
        return (m < 10 ? "0" : "") + m + ":" + (s < 10 ? "0" : "") + s
    }
}
