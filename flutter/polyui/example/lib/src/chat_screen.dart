import 'package:flutter/material.dart';
import 'package:polyui/polyui.dart';

import 'chat_model.dart';
import 'demo_script.dart';
import 'speech.dart';

typedef VoiceFactory = PolyuiVoice Function(ChatModel model);

PolyuiVoice deviceVoice(ChatModel model) => PolyuiVoice(
  session: model.session,
  output: TtsOutput(),
  input: RecognizerInput(),
  locale: ChatModel.hello.locale ?? 'en',
  onUnmatched: model.ask,
  onMatched: (transcript, match) => model.spokenMatch(transcript),
);

class ChatScreen extends StatefulWidget {
  const ChatScreen({
    super.key,
    required this.model,
    required this.demo,
    this.voiceFactory = deviceVoice,
  });

  final ChatModel model;
  final DemoScript demo;
  final VoiceFactory voiceFactory;

  @override
  State<ChatScreen> createState() => ChatScreenState();
}

class ChatScreenState extends State<ChatScreen> {
  final TextEditingController draft = TextEditingController();
  final ScrollController scroll = ScrollController();
  int seenEntries = 0;
  PolyuiVoice? currentVoice;
  bool voiceDemoDone = false;

  ChatModel get model => widget.model;

  PolyuiVoice get voice {
    final existing = currentVoice;
    if (existing != null && existing.session == model.session) {
      return existing;
    }
    existing?.removeListener(voiceChanged);
    existing?.dispose();
    final made = widget.voiceFactory(model);
    made.addListener(voiceChanged);
    currentVoice = made;
    return made;
  }

  void voiceChanged() {
    if (model.log) {
      final active = currentVoice;
      debugPrint(
        'polyui-example voice: ${active?.state.name} '
        'problem=${active?.problem?.name}',
      );
    }
    if (mounted) {
      setState(() {});
    }
  }

  void runVoiceDemo() {
    final step = widget.demo.voiceStep;
    final surface = model.latestSurface;
    if (voiceDemoDone || step.isEmpty || surface == null) {
      return;
    }
    if (model.session.surface(surface).rootId == null) {
      Future<void>.delayed(const Duration(milliseconds: 300), () {
        if (mounted) {
          runVoiceDemo();
        }
      });
      return;
    }
    voiceDemoDone = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      switch (step) {
        case 'read':
          readAloud();
        case 'listen':
          listen();
      }
    });
  }

  Future<void> listen() async {
    final active = voice;
    if (active.state == PolyuiVoiceState.listening) {
      await active.stopListening();
      return;
    }
    final surface = model.latestSurface ?? '';
    await active.listen(surface);
  }

  Future<void> readAloud() async {
    final active = voice;
    if (active.state == PolyuiVoiceState.speaking) {
      await active.stopSpeaking();
      return;
    }
    final surface = model.latestSurface;
    if (surface != null) {
      await active.speak(surface);
    }
  }

  Future<void> chooseLive() async {
    final url = await showDialog<Uri>(
      context: context,
      builder: (context) => LiveDialog(initial: model.liveUrl),
    );
    if (url != null) {
      model.goLive(url);
    }
  }

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
    currentVoice?.removeListener(voiceChanged);
    currentVoice?.dispose();
    draft.dispose();
    scroll.dispose();
    super.dispose();
  }

  void changed() {
    setState(() {});
    runVoiceDemo();
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
            ChatHeader(
              connection: model.connection,
              speaking: voice.state == PolyuiVoiceState.speaking,
              canRead: model.latestSurface != null,
              onReplay: model.replay,
              onLive: chooseLive,
              onRead: readAloud,
            ),
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
                    if (model.entries.isEmpty)
                      Suggestions(
                        questions: [model.question, model.weatherQuestion],
                        onAsk: model.ask,
                      ),
                    if (model.problem case final problem?)
                      Text(problem, style: TextStyle(color: theme.negative)),
                  ],
                ),
              ),
            ),
            if (voice.problem case final trouble?)
              VoiceNotice(problem: trouble, onClose: voice.clearProblem),
            SafeArea(
              top: false,
              child: Composer(
                controller: draft,
                onSend: send,
                listening: voice.state == PolyuiVoiceState.listening,
                onListen: listen,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class ChatHeader extends StatelessWidget {
  const ChatHeader({
    super.key,
    required this.connection,
    required this.speaking,
    required this.canRead,
    required this.onReplay,
    required this.onLive,
    required this.onRead,
  });

  final Connection connection;
  final bool speaking;
  final bool canRead;
  final VoidCallback onReplay;
  final VoidCallback onLive;
  final VoidCallback onRead;

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
    final (label, dot) = switch (connection) {
      Connection.replay => ('Replay', theme.accent),
      Connection.connecting => ('Connecting', theme.muted),
      Connection.live => ('Live', theme.positive),
      Connection.offline => ('Offline', theme.negative),
    };
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
          IconButton(
            tooltip: speaking ? 'Stop reading' : 'Read the answer aloud',
            onPressed: canRead || speaking ? onRead : null,
            icon: Icon(
              speaking ? Icons.stop_circle_outlined : Icons.volume_up_outlined,
              color: theme.muted,
            ),
          ),
          PopupMenuButton<ChatMode>(
            tooltip: 'Connection',
            onSelected: (mode) => switch (mode) {
              ChatMode.replay => onReplay(),
              ChatMode.live => onLive(),
            },
            itemBuilder: (context) => const [
              PopupMenuItem(
                value: ChatMode.replay,
                child: ListTile(
                  leading: Icon(Icons.replay),
                  title: Text('Replay the demo'),
                ),
              ),
              PopupMenuItem(
                value: ChatMode.live,
                child: ListTile(
                  leading: Icon(Icons.cable),
                  title: Text('Connect to polyui serve'),
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
                      color: dot,
                      shape: BoxShape.circle,
                    ),
                  ),
                  Text(
                    label,
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

class LiveDialog extends StatefulWidget {
  const LiveDialog({super.key, required this.initial});

  final Uri initial;

  @override
  State<LiveDialog> createState() => LiveDialogState();
}

class LiveDialogState extends State<LiveDialog> {
  late final TextEditingController address = TextEditingController(
    text: widget.initial.toString(),
  );
  String? error;

  @override
  void dispose() {
    address.dispose();
    super.dispose();
  }

  void connect() {
    final parsed = Uri.tryParse(address.text.trim());
    final valid =
        parsed != null &&
        (parsed.scheme == 'ws' || parsed.scheme == 'wss') &&
        parsed.host.isNotEmpty;
    if (!valid) {
      setState(() => error = 'Use a ws:// or wss:// address.');
      return;
    }
    Navigator.of(context).pop(parsed);
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Connect to polyui serve'),
    content: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: 12,
      children: [
        const Text(
          'Start the server on your computer with polyui serve. '
          'The Android emulator reaches it at 10.0.2.2.',
        ),
        TextField(
          controller: address,
          autocorrect: false,
          keyboardType: TextInputType.url,
          onSubmitted: (_) => connect(),
          decoration: InputDecoration(
            labelText: 'WebSocket address',
            errorText: error,
          ),
        ),
      ],
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Cancel'),
      ),
      FilledButton(onPressed: connect, child: const Text('Connect')),
    ],
  );
}

class Suggestions extends StatelessWidget {
  const Suggestions({super.key, required this.questions, required this.onAsk});

  final List<String> questions;
  final ValueChanged<String> onAsk;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: 8,
    runSpacing: 8,
    children: [
      for (final question in questions)
        ActionChip(label: Text(question), onPressed: () => onAsk(question)),
    ],
  );
}

class VoiceNotice extends StatelessWidget {
  const VoiceNotice({super.key, required this.problem, required this.onClose});

  final PolyuiVoiceProblem problem;
  final VoidCallback onClose;

  String get message => switch (problem) {
    PolyuiVoiceProblem.microphoneDenied =>
      'Microphone access is off, so voice input cannot start. '
          'Turn it on in Settings, or keep using touch.',
    PolyuiVoiceProblem.recognitionUnavailable =>
      'Speech recognition is not available on this device. '
          'Keep using touch and the keyboard.',
    PolyuiVoiceProblem.speechFailed =>
      'The answer could not be read aloud. Everything is still on screen.',
  };

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    return Semantics(
      liveRegion: true,
      child: Container(
        margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
        padding: const EdgeInsets.fromLTRB(14, 10, 4, 10),
        decoration: BoxDecoration(
          color: theme.sunken,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: theme.border),
        ),
        child: Row(
          spacing: 8,
          children: [
            Icon(Icons.mic_off_outlined, color: theme.muted, size: 20),
            Expanded(
              child: Text(
                message,
                style: TextStyle(fontSize: 15, color: theme.foreground),
              ),
            ),
            IconButton(
              tooltip: 'Dismiss',
              onPressed: onClose,
              icon: Icon(Icons.close, size: 18, color: theme.muted),
            ),
          ],
        ),
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
      UserEntry(:final text, :final spoken) => Align(
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
              child: Row(
                mainAxisSize: MainAxisSize.min,
                spacing: 6,
                children: [
                  if (spoken)
                    Icon(Icons.mic_none, size: 16, color: theme.muted),
                  Flexible(child: Text(text, style: body)),
                ],
              ),
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
  const Composer({
    super.key,
    required this.controller,
    required this.onSend,
    required this.listening,
    required this.onListen,
  });

  final TextEditingController controller;
  final VoidCallback onSend;
  final bool listening;
  final VoidCallback onListen;

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
                  hintText: listening ? 'Listening' : 'Message the assistant',
                  hintStyle: TextStyle(fontSize: 17, color: theme.muted),
                  border: InputBorder.none,
                  contentPadding: const EdgeInsets.fromLTRB(16, 12, 8, 12),
                ),
              ),
            ),
            IconButton(
              key: const ValueKey('voice-input'),
              tooltip: listening ? 'Stop listening' : 'Speak',
              onPressed: onListen,
              icon: Icon(
                listening ? Icons.mic : Icons.mic_none,
                color: listening ? theme.accent : theme.muted,
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
