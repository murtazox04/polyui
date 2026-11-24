package dev.polyui

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot

@Stable
class PolyuiSurfaceModel(
    val id: String,
) {
    private val nodes = mutableStateMapOf<String, ComponentNode>()

    var rootId: String? by mutableStateOf(null)
        private set

    var highlightedId: String? by mutableStateOf(null)
        private set

    var pendingConfirm: ConfirmRequest? by mutableStateOf(null)
        internal set

    var isClosed: Boolean by mutableStateOf(false)
        private set

    var fallback: String? by mutableStateOf(null)
        private set

    var structureRevision: Int by mutableIntStateOf(0)
        private set

    val root: ComponentNode?
        get() = rootId?.let(nodes::get)

    val nodeIds: Set<String>
        get() = nodes.keys.toSet()

    fun node(id: String): ComponentNode? = nodes[id]

    fun apply(batch: RenderBatch) {
        Snapshot.withMutableSnapshot { applyNow(batch) }
    }

    private fun applyNow(batch: RenderBatch) {
        batch.fallback?.let { fallback = it }
        val recreated = batch.ops.mapNotNullTo(HashSet()) { (it as? RenderOp.Create)?.payload?.id }
        var structural = false
        for (op in batch.ops) {
            when (op) {
                is RenderOp.Create -> {
                    create(op.payload)
                    structural = true
                }

                is RenderOp.Update -> {
                    nodes[op.payload.id]?.update(op.payload)
                }

                is RenderOp.Children -> {
                    val parent = nodes[op.id] ?: continue
                    val kept = op.children.filter { it != op.id }
                    if (parent.children != kept) {
                        parent.children = kept
                        structural = true
                    }
                }

                is RenderOp.Remove -> {
                    if (op.id in recreated) continue
                    remove(op.id)
                    structural = true
                }

                is RenderOp.Root -> {
                    if (nodes[op.id] == null || rootId == op.id) continue
                    rootId = op.id
                    structural = true
                }

                is RenderOp.Confirm -> {
                    pendingConfirm = op.request
                }

                is RenderOp.Highlight -> {
                    highlight(op.id)
                }

                RenderOp.Close -> {
                    close()
                    structural = true
                }

                is RenderOp.Unknown -> {
                    continue
                }
            }
        }
        if (structural) structureRevision += 1
    }

    internal fun highlight(id: String?) {
        if (highlightedId == id) return
        highlightedId?.let { nodes[it]?.isHighlighted = false }
        val target = id?.let(nodes::get)
        target?.isHighlighted = true
        highlightedId = target?.id
    }

    private fun create(payload: ComponentPayload) {
        isClosed = false
        val type = payload.type ?: "Text"
        val existing = nodes[payload.id]
        if (existing != null && existing.type == type) {
            existing.update(payload)
            return
        }
        if (highlightedId == payload.id) highlightedId = null
        nodes[payload.id] = ComponentNode(payload, type)
    }

    private fun remove(id: String) {
        if (nodes.remove(id) == null) return
        if (rootId == id) rootId = null
        if (highlightedId == id) highlightedId = null
    }

    private fun close() {
        nodes.clear()
        rootId = null
        highlightedId = null
        pendingConfirm = null
        isClosed = true
    }
}
