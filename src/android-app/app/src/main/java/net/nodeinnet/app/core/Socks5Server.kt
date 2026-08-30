package net.nodeinnet.app.core

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import org.json.JSONArray

class Socks5Server(
    private val port: Int,
    private val resourceId: String,
    private val peerId: String,
    private val sendP2pCallback: (String, String) -> Unit
) {
    private var serverSocket: ServerSocket? = null
    private var serverJob: Job? = null
    private val scope = CoroutineScope(Dispatchers.IO)
    
    private val activeStreams = ConcurrentHashMap<String, Socket>()
    private val pendingConnects = ConcurrentHashMap<String, Socket>()

    fun start() {
        serverJob = scope.launch {
            try {
                serverSocket = ServerSocket(port, 50, InetAddress.getByName("127.0.0.1"))
                Log.d("Socks5Server", "SOCKS5 Proxy started locally on 127.0.0.1:$port")
                
                while (isActive) {
                    val socket = serverSocket?.accept() ?: break
                    launch { handleSocket(socket) }
                }
            } catch (e: Exception) {
                Log.e("Socks5Server", "SOCKS5 Server Exception: \${e.message}")
            }
        }
    }

    fun stop() {
        serverJob?.cancel()
        serverSocket?.close()
        activeStreams.values.forEach { try { it.close() } catch (e: Exception) {} }
        pendingConnects.values.forEach { try { it.close() } catch (e: Exception) {} }
        activeStreams.clear()
        pendingConnects.clear()
    }

    fun handleConnectResponse(streamId: String, isSuccess: Boolean) {
        val socket = pendingConnects.remove(streamId) ?: return
        if (isSuccess) {
            try {
                val out = socket.getOutputStream()
                val reply = byteArrayOf(0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0)
                out.write(reply)
                out.flush()
                
                activeStreams[streamId] = socket
                
                scope.launch {
                    val input = socket.getInputStream()
                    val buffer = ByteArray(16384)
                    try {
                        while (isActive && !socket.isClosed) {
                            val bytesRead = input.read(buffer)
                            if (bytesRead == -1) break
                            
                            val jsonArray = JSONArray()
                            for (i in 0 until bytesRead) {
                                jsonArray.put(buffer[i].toInt() and 0xFF)
                            }
                            val req = """{"cmd":"SocksData","data":{"resource_id":"$resourceId","stream_id":"$streamId","data":$jsonArray}}"""
                            sendP2pCallback(peerId, req)
                        }
                    } catch (e: Exception) {
                        Log.e("Socks5Server", "Error reading proxy socket: \${e.message}")
                    } finally {
                        val closeReq = """{"cmd":"SocksClose","data":{"resource_id":"$resourceId","stream_id":"$streamId"}}"""
                        sendP2pCallback(peerId, closeReq)
                        activeStreams.remove(streamId)
                        socket.close()
                    }
                }
            } catch (e: Exception) {
                socket.close()
            }
        } else {
            try {
                val out = socket.getOutputStream()
                val reply = byteArrayOf(0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0)
                out.write(reply)
                out.flush()
            } catch (e: Exception) {}
            socket.close()
        }
    }

    fun feedData(streamId: String, data: ByteArray) {
        val socket = activeStreams[streamId] ?: return
        scope.launch {
            try {
                socket.getOutputStream().write(data)
                socket.getOutputStream().flush()
            } catch (e: Exception) {
                closeStream(streamId)
            }
        }
    }

    fun closeStream(streamId: String) {
        activeStreams.remove(streamId)?.close()
        pendingConnects.remove(streamId)?.close()
    }

    private suspend fun handleSocket(socket: Socket) {
        withContext(Dispatchers.IO) {
            try {
                val input = socket.getInputStream()
                val output = socket.getOutputStream()
                
                val header = ByteArray(2)
                readExact(input, header)
                if (header[0] != 0x05.toByte()) {
                    socket.close()
                    return@withContext
                }
                
                val numMethods = header[1].toInt() and 0xFF
                val methods = ByteArray(numMethods)
                readExact(input, methods)
                
                output.write(byteArrayOf(0x05, 0x00))
                output.flush()
                
                val reqHeader = ByteArray(4)
                readExact(input, reqHeader)
                if (reqHeader[0] != 0x05.toByte() || reqHeader[1] != 0x01.toByte()) {
                    socket.close()
                    return@withContext
                }
                
                val atyp = reqHeader[3].toInt() and 0xFF
                val host = when (atyp) {
                    0x01 -> {
                        val ipBytes = ByteArray(4)
                        readExact(input, ipBytes)
                        InetAddress.getByAddress(ipBytes).hostAddress
                    }
                    0x03 -> {
                        val lenByte = ByteArray(1)
                        readExact(input, lenByte)
                        val domainLen = lenByte[0].toInt() and 0xFF
                        val domainBytes = ByteArray(domainLen)
                        readExact(input, domainBytes)
                        String(domainBytes, Charsets.UTF_8)
                    }
                    0x04 -> {
                        val ipBytes = ByteArray(16)
                        readExact(input, ipBytes)
                        InetAddress.getByAddress(ipBytes).hostAddress
                    }
                    else -> {
                        socket.close()
                        return@withContext
                    }
                }
                
                val portBytes = ByteArray(2)
                readExact(input, portBytes)
                val portInt = ((portBytes[0].toInt() and 0xFF) shl 8) or (portBytes[1].toInt() and 0xFF)
                
                val streamId = UUID.randomUUID().toString()
                pendingConnects[streamId] = socket
                
                val p2pReq = """{"cmd":"SocksConnectRequest","data":{"resource_id":"$resourceId","stream_id":"$streamId","host":"$host","port":$portInt}}"""
                sendP2pCallback(peerId, p2pReq)
                
            } catch (e: Exception) {
                Log.e("Socks5Server", "Handshake failed: \${e.message}")
                socket.close()
            }
        }
    }
    
    private fun readExact(input: InputStream, buffer: ByteArray) {
        var totalRead = 0
        while (totalRead < buffer.size) {
            val read = input.read(buffer, totalRead, buffer.size - totalRead)
            if (read == -1) throw Exception("EOF")
            totalRead += read
        }
    }
}
