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
}
