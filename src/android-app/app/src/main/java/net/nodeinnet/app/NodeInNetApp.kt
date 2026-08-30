package net.nodeinnet.app

import android.app.Application
import android.content.Context
import net.nodeinnet.app.core.Locales

class NodeInNetApp : Application() {
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(Locales.wrap(base))
    }
}
