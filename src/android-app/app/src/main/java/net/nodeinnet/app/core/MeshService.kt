package net.nodeinnet.app.core

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import net.nodeinnet.app.MainActivity
import net.nodeinnet.app.R

class MeshService : android.app.Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val summary = intent?.getStringExtra(EXTRA_SUMMARY) ?: getString(R.string.fg_default)
        startAsForeground(summary)
        return START_STICKY
    }

    private fun startAsForeground(summary: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, getString(R.string.fg_channel_name), NotificationManager.IMPORTANCE_LOW)
                    .apply { description = getString(R.string.fg_channel_desc) }
            )
        }

        val tap = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val notification: Notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Node.In.Net")
            .setContentText(summary)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setContentIntent(tap)
            .build()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    companion object {
        private const val CHANNEL_ID = "nodeinnet_shared_services"
        private const val NOTIFICATION_ID = 1001
        private const val EXTRA_SUMMARY = "summary"

        fun start(ctx: Context, summary: String) {
            val intent = Intent(ctx, MeshService::class.java).putExtra(EXTRA_SUMMARY, summary)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                ctx.startForegroundService(intent)
            } else {
                ctx.startService(intent)
            }
        }

        fun stop(ctx: Context) {
            ctx.stopService(Intent(ctx, MeshService::class.java))
        }
    }
}
