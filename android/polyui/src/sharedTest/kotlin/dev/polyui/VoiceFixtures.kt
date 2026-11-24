package dev.polyui

object VoiceFixtures {
    const val OPEN =
        """{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"card","components":[
          {"id":"card","type":"Card","props":{"title":"Your bills"},"children":["choices"]},
          {"id":"choices","type":"ChoiceGroup","props":{"label":"Pick one","value":"/pick","options":[{"id":"gas","label":"Gas"},{"id":"power","label":"Power"}],
           "on_change":{"action_id":"act_pick","confirm":false,"with":["/pick"]}}}
        ],"data":{}}"""
}
