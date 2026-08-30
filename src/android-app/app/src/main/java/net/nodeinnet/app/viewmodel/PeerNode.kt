package net.nodeinnet.app.viewmodel

data class PeerNode(
    val id: String,
    val name: String,
    val os: String,
    val publicKey: String,
    val resourcesJson: String,
    val isOnline: Boolean = true,
    val lastUsed: Long = 0L,
)
