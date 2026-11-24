import 'package:flutter/material.dart';
import 'package:polyui/polyui.dart';

import 'chat_model.dart';
import 'demo_script.dart';

class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key, required this.model, required this.demo});

  final ChatModel model;
  final DemoScript demo;

  @override
  State<ChatScreen> createState() => ChatScreenState();
}

class ChatScreenState extends State<ChatScreen> {
  final TextEditingController draft = TextEditingController();
  final ScrollController scroll = ScrollController();
  int seenEntries = 0;

  ChatModel get model => widget.model;

  @override
  void initState() {
    super.initState();
    model.addListener(changed);
    draft.addListener(() => setState(() {}));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      widget.demo.run(model);
    });
  }

  @override
  void dispose() {
    model.removeListener(changed);
    draft.dispose();
    scroll.dispose();
    super.dispose();
  }

  void changed() {
    setState(() {});
    if (!widget.demo.scrollToBottom || model.entries.length == seenEntries) {
      return;
    }
    seenEntries = model.entries.length;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (scroll.hasClients) {
        scroll.jumpTo(scroll.position.maxScrollExtent);
      }
    });
  }

  void send() {
    final text = draft.text;
    draft.clear();
    model.ask(text);
  }

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: Column(
          children: [
            ChatHeader(onReplay: model.replay),
            Divider(height: 1, thickness: 1, color: theme.border),
            Expanded(
              child: SingleChildScrollView(
                controller: scroll,
                keyboardDismissBehavior:
                    ScrollViewKeyboardDismissBehavior.onDrag,
                padding: const EdgeInsets.fromLTRB(16, 20, 16, 20),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  spacing: 18,
                  children: [
                    for (final entry in model.entries)
                      KeyedSubtree(
                        key: ValueKey(entry.id),
                        child: ChatRow(entry: entry, session: model.session),
                      ),
                    if (model.problem case final problem?)
                      Text(problem, style: TextStyle(color: theme.negative)),
                  ],
                ),
              ),
            ),
            SafeArea(
              top: false,
              child: Composer(controller: draft, onSend: send),
            ),
          ],
        ),
      ),
    );
  }
}

class ChatHeader extends StatelessWidget {
  const ChatHeader({super.key, required this.onReplay});

  final VoidCallback onReplay;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    Widget tile(Color color) => Container(
      width: 8.5,
      height: 8.5,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(2.5),
      ),
    );
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Row(
        spacing: 10,
        children: [
          ExcludeSemantics(
            child: SizedBox(
              width: 20,
              height: 20,
              child: Column(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [tile(theme.series[0]), tile(theme.series[1])],
                  ),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [tile(theme.series[1]), tile(theme.series[0])],
                  ),
                ],
              ),
            ),
          ),
          Text(
            'PolyUI',
            style: TextStyle(
              fontSize: 17,
              fontWeight: FontWeight.w600,
              color: theme.foreground,
            ),
          ),
          const Spacer(),
          PopupMenuButton<String>(
            tooltip: 'Connection',
            onSelected: (_) => onReplay(),
            itemBuilder: (context) => const [
              PopupMenuItem(
                value: 'replay',
                child: ListTile(
                  leading: Icon(Icons.replay),
                  title: Text('Replay the demo'),
                ),
              ),
            ],
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              decoration: ShapeDecoration(
                color: theme.raised,
                shape: StadiumBorder(side: BorderSide(color: theme.border)),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                spacing: 6,
                children: [
                  Container(
                    width: 8,
                    height: 8,
                    decoration: BoxDecoration(
                      color: theme.accent,
                      shape: BoxShape.circle,
                    ),
                  ),
                  Text(
                    'Replay',
                    style: TextStyle(fontSize: 15, color: theme.muted),
                  ),
                  Icon(Icons.expand_more, size: 18, color: theme.muted),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class ChatRow extends StatelessWidget {
  const ChatRow({super.key, required this.entry, required this.session});

  final ChatEntry entry;
  final PolyuiSession session;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final body = TextStyle(fontSize: 17, height: 1.35, color: theme.foreground);
    return switch (entry) {
      UserEntry(:final text) => Align(
        alignment: AlignmentDirectional.centerEnd,
        child: Padding(
          padding: const EdgeInsetsDirectional.only(start: 48),
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: theme.accent.withValues(alpha: 0.14),
              borderRadius: BorderRadius.circular(18),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              child: Text(text, style: body),
            ),
          ),
        ),
      ),
      AgentEntry(:final text) => Text(text, style: body),
      SurfaceEntry(:final surface) => PolyuiSurface(
        session: session,
        surface: surface,
      ),
    };
  }
}

class Composer extends StatelessWidget {
  const Composer({super.key, required this.controller, required this.onSend});

  final TextEditingController controller;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final empty = controller.text.trim().isEmpty;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: theme.raised,
          borderRadius: BorderRadius.circular(24),
          border: Border.all(color: theme.border),
        ),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                controller: controller,
                minLines: 1,
                maxLines: 4,
                textInputAction: TextInputAction.send,
                onSubmitted: (_) => onSend(),
                style: TextStyle(fontSize: 17, color: theme.foreground),
                decoration: InputDecoration(
                  hintText: 'Message the assistant',
                  hintStyle: TextStyle(fontSize: 17, color: theme.muted),
                  border: InputBorder.none,
                  contentPadding: const EdgeInsets.fromLTRB(16, 12, 8, 12),
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.only(right: 6),
              child: IconButton.filled(
                tooltip: 'Send',
                onPressed: empty ? null : onSend,
                style: IconButton.styleFrom(
                  backgroundColor: theme.accent,
                  foregroundColor: theme.accentForeground,
                  disabledBackgroundColor: theme.borderStrong,
                  disabledForegroundColor: theme.accentForeground,
                  fixedSize: const Size(36, 36),
                  minimumSize: const Size(36, 36),
                  padding: EdgeInsets.zero,
                ),
                icon: const Icon(Icons.arrow_upward, size: 20),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
