package net.nodeinnet.app.core

class MouseTracker {
    var tracking = false
        private set
    var sgr = false
        private set
    private enum class State { IDLE, ESC, CSI, PARAMS }

    private var state = State.IDLE
    private val params = StringBuilder()

     
    fun consume(data: ByteArray, length: Int = data.size) {
        for (i in 0 until length) {
            val c = data[i].toInt().toChar()
            when (state) {
                State.IDLE -> if (data[i].toInt() == 0x1B) state = State.ESC
                State.ESC -> state = if (c == '[') State.CSI else State.IDLE
                State.CSI ->
                    if (c == '?') {
                        params.setLength(0)
                        state = State.PARAMS
                    } else {
                        state = State.IDLE
                    }
                State.PARAMS -> when {
                    c.isDigit() || c == ';' -> if (params.length < 64) params.append(c)
                    c == 'h' || c == 'l' -> {
                        applyModes(params.toString(), on = c == 'h')
                        state = State.IDLE
                    }
                    else -> state = State.IDLE
                }
            }
        }
    }

    private fun applyModes(csvParams: String, on: Boolean) {
        for (part in csvParams.split(';')) {
            when (part.toIntOrNull()) {
                1000, 1002, 1003 -> tracking = on
                1006 -> sgr = on
            }
        }
    }

     
    fun clickReport(col: Int, row: Int): ByteArray =
        if (sgr) {
            "\u001b[<0;$col;${row}M\u001b[<0;$col;${row}m".toByteArray(Charsets.ISO_8859_1)
        } else {
            val cb = (32 + col).coerceAtMost(255)
            val rb = (32 + row).coerceAtMost(255)
            byteArrayOf(
                0x1B, '['.code.toByte(), 'M'.code.toByte(),
                32, cb.toByte(), rb.toByte(),
                0x1B, '['.code.toByte(), 'M'.code.toByte(),
                (32 + 3).toByte(), cb.toByte(), rb.toByte(),
            )
        }
}
