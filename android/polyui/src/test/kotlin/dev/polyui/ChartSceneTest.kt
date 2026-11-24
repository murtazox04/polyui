package dev.polyui

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ChartSceneTest {
    private fun fixtureScene(): ChartScene {
        val path = requireNotNull(System.getProperty("polyui.conversation")) { "polyui.conversation is not set" }
        val conversation = Json.parseToJsonElement(File(path).readText())
        val scene =
            conversation["open"]["components"].arrayValue?.firstNotNullOfOrNull { component ->
                component["props"]["scene"]?.let(ChartScene::read)
            }
        return requireNotNull(scene) { "the conversation has no chart scene" }
    }

    @Test
    fun placeholderScenesAreRefused() {
        val fixture = Json.parseToJsonElement(File(ConformanceTest.folder, "chart_scene_and_detail.json").readText())
        val scene =
            fixture["steps"]
                .arrayValue
                ?.first()["expect"]["ops"]
                .arrayValue
                ?.first()["props"]["scene"]
        assertNotNull(scene)
        assertNull(ChartScene.read(scene))
    }

    @Test
    fun readsTheServerScene() {
        val scene = fixtureScene()
        assertTrue(scene.marks.isNotEmpty())
        assertTrue(scene.hits.isNotEmpty())
        assertTrue(scene.viewBox.width > 0f)
    }

    @Test
    fun hitTestingFindsTheRowUnderAPoint() {
        val scene = fixtureScene()
        scene.hits.forEach { hit ->
            val bounds = hit.bounds
            val centre = ScenePoint(bounds.x + bounds.width / 2f, bounds.y + bounds.height / 2f)
            assertEquals(hit.row, scene.hitAt(centre)?.row)
        }
        assertNull(scene.hitAt(ScenePoint(-10f, -10f)))
    }

    @Test
    fun polygonsUseTheirRealShape() {
        val triangle =
            ChartScene.Hit(
                row = 2,
                series = null,
                label = "slice",
                shape = ChartScene.HitShape.Polygon(listOf(ScenePoint(0f, 0f), ScenePoint(10f, 0f), ScenePoint(0f, 10f))),
                index = 0,
            )
        assertTrue(triangle.contains(ScenePoint(2f, 2f)))
        assertTrue(!triangle.contains(ScenePoint(9f, 9f)))
    }

    @Test
    fun rejectsAnEmptyViewBox() {
        assertNull(ChartScene.read(Json.parseToJsonElement("""{"view_box":{"x":0,"y":0,"width":0,"height":10},"marks":[]}""")))
        assertNotNull(ChartScene.read(Json.parseToJsonElement("""{"view_box":{"x":0,"y":0,"width":10,"height":10},"marks":[]}""")))
    }
}
