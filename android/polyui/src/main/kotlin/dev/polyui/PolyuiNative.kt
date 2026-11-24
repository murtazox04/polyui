package dev.polyui

internal object PolyuiNative {
    const val LIBRARY_PATH_PROPERTY = "polyui.library"

    init {
        val explicit = System.getProperty(LIBRARY_PATH_PROPERTY)
        if (explicit.isNullOrEmpty()) {
            System.loadLibrary("polyui_jni")
        } else {
            System.load(explicit)
        }
    }

    @JvmStatic
    external fun clientNew(
        hello: ByteArray,
        allowHttp: Boolean,
        error: Array<ByteArray?>,
    ): Long

    @JvmStatic
    external fun clientNewWithCatalogs(
        hello: ByteArray,
        catalogs: ByteArray,
        allowHttp: Boolean,
        error: Array<ByteArray?>,
    ): Long

    @JvmStatic
    external fun receive(
        client: Long,
        message: ByteArray,
        error: Array<ByteArray?>,
    ): ByteArray?

    @JvmStatic
    external fun setValue(
        client: Long,
        surface: ByteArray,
        path: ByteArray,
        value: ByteArray,
        error: Array<ByteArray?>,
    ): ByteArray?

    @JvmStatic
    external fun fire(
        client: Long,
        surface: ByteArray,
        component: ByteArray,
        prop: ByteArray,
        detail: ByteArray?,
        error: Array<ByteArray?>,
    ): ByteArray?

    @JvmStatic
    external fun confirm(
        client: Long,
        surface: ByteArray,
        actionId: ByteArray,
        nonce: ByteArray,
        error: Array<ByteArray?>,
    ): ByteArray?

    @JvmStatic
    external fun say(
        client: Long,
        surface: ByteArray,
        error: Array<ByteArray?>,
    ): ByteArray?

    @JvmStatic
    external fun matchVoice(
        client: Long,
        surface: ByteArray,
        text: ByteArray,
        error: Array<ByteArray?>,
    ): ByteArray?

    @JvmStatic
    external fun clientFree(client: Long)
}
