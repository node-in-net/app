package net.nodeinnet.app.core

import android.util.Log

interface NodeCallback {
    fun onLog(msg: String)
    fun onConnected()
    fun onDisconnected()
    fun onP2pConnected(peerId: String)
    fun onP2pDisconnected(peerId: String)

    fun onPeerState(json: String)
    fun onUpdateNodes(json: String)
    fun onP2pMessage(json: String)
    fun onUpdateAvailable(json: String)
    fun onRemoteDesktopFrame(resourceId: String, bgraData: ByteArray, width: Int, height: Int)

    fun onRemoteDesktopStopped(resourceId: String)

    fun onTransferProgress(transferId: String, bytes: Long, total: Long)

    fun onTransferComplete(transferId: String, isUpload: Boolean)
}

object NativeNode {
    private const val TAG = "NodeInNet-Native"
    init {
        System.loadLibrary("android_node")
        Log.i(TAG, "Successfully loaded libandroid_node.so")
    }

     
    @JvmStatic
    external fun testNativeLoad(): String
    @JvmStatic
    external fun generateKeyPair(): String

     
    @JvmStatic
    external fun encodeResourcesToBsonBase64(nodeInfoJson: String): String
    @JvmStatic
    external fun connectNode(
        nodeInfoJson: String,
        wsUrl: String,
        privateKeyBase64: String,
        turnCredentialsJson: String,
        jCallbackObj: Any
    )

    @JvmStatic
    external fun reconnectNode(
        nodeInfoJson: String,
        wsUrl: String,
        turnCredentialsJson: String,
    )

    @JvmStatic
    external fun applyTurnCredentials(turnCredentialsJson: String)

     
    @JvmStatic
    external fun sendP2pMessage(peerId: String, messageJson: String)

     
    @JvmStatic
    external fun callPeer(peerId: String)
    @JvmStatic
    external fun broadcastP2pMessage(targetResourceType: String, messageJson: String)
     
    @JvmStatic
    external fun updateResources(resourcesJson: String)

     
    @JvmStatic
    external fun setTempDir(dir: String)

     
    @JvmStatic
    external fun startDownload(peerId: String, resourceId: String, remotePath: String): String
    @JvmStatic
    external fun startUpload(
        peerId: String,
        resourceId: String,
        localPath: String,
        targetPath: String,
        fileName: String,
    ): String
}
