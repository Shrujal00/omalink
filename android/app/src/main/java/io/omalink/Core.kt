package io.omalink

/** The Rust core: the wire protocol shared with the desktop. */
object Core {
    init {
        System.loadLibrary("omalink_core")
    }

    @JvmStatic external fun serviceUuid(): String
    @JvmStatic external fun protocolVersion(): Int
    @JvmStatic external fun newCodec(): Long
    @JvmStatic external fun freeCodec(handle: Long)

    /** Whole messages completed by these bytes, as JSON; null if the stream is corrupt. */
    @JvmStatic external fun feed(handle: Long, bytes: ByteArray, length: Int): Array<String>?

    /** The message framed for the wire; null if it isn't one the desktop understands. */
    @JvmStatic external fun frame(json: String): ByteArray?
    @JvmStatic external fun explain(json: String): String
    @JvmStatic external fun numberKey(number: String): String

    // The network link; see android/core/src/net.rs.

    /** Starts networking; this phone's address, or null if it couldn't. */
    @JvmStatic external fun netStart(context: android.content.Context, dir: String, name: String): String?
    /** The next event as JSON, or null after the timeout. */
    @JvmStatic external fun netNext(timeoutMs: Int): String?
    @JvmStatic external fun netConnected(): Boolean
    @JvmStatic external fun netSend(json: String): Boolean
    @JvmStatic external fun netPair(id: String, name: String)
    @JvmStatic external fun netChoose(index: Int)
    @JvmStatic external fun netDecline()
    @JvmStatic external fun netPeers(): String
    @JvmStatic external fun netForget(id: String)
    @JvmStatic external fun netNetworkChanged()
    @JvmStatic external fun netInvited(id: String, name: String)
    @JvmStatic external fun netOpenFile(id: Long, name: String, size: Long): Long
    @JvmStatic external fun netOpenMirror(width: Int, height: Int): Long
    @JvmStatic external fun netWrite(handle: Long, bytes: ByteArray, length: Int): Boolean
    @JvmStatic external fun netWriteFrame(handle: Long, bytes: ByteArray, length: Int, ptsUs: Long, flags: Int): Boolean
    @JvmStatic external fun netFinish(handle: Long): Boolean
    /** Bytes read into the buffer; 0 at the end, -1 on error. */
    @JvmStatic external fun netRead(handle: Long, buffer: ByteArray): Int
    @JvmStatic external fun netClose(handle: Long)
}
