import 'dart:async';

import 'package:flutter/material.dart';

import '../core/render_batch.dart';
import 'theme.dart';

class PolyuiConfirmRequestSheet extends StatefulWidget {
  const PolyuiConfirmRequestSheet({
    super.key,
    required this.request,
    required this.labels,
    required this.delay,
  });

  final ConfirmRequest request;
  final PolyuiLabels labels;
  final Duration delay;

  @override
  State<PolyuiConfirmRequestSheet> createState() =>
      PolyuiConfirmRequestSheetState();
}

class PolyuiConfirmRequestSheetState extends State<PolyuiConfirmRequestSheet>
    with SingleTickerProviderStateMixin {
  late final AnimationController countdown;
  Timer? armTimer;
  Timer? expiryTimer;
  bool armed = false;
  bool expired = false;
  bool answered = false;

  int? get expiresMs {
    final value = widget.request.expiresMs;
    return value != null && value > 0 ? value : null;
  }

  @override
  void initState() {
    super.initState();
    final expires = expiresMs;
    countdown = AnimationController(
      vsync: this,
      value: 1,
      duration: Duration(milliseconds: expires ?? 1),
      animationBehavior: AnimationBehavior.preserve,
    );
    armTimer = Timer(widget.delay, () {
      if (mounted) {
        setState(() => armed = true);
      }
    });
    if (expires != null) {
      countdown.reverse(from: 1);
      expiryTimer = Timer(Duration(milliseconds: expires), () {
        if (mounted) {
          setState(() => expired = true);
        }
      });
    }
  }

  @override
  void dispose() {
    armTimer?.cancel();
    expiryTimer?.cancel();
    countdown.dispose();
    super.dispose();
  }

  void confirm() {
    if (!armed || expired || answered) {
      return;
    }
    answered = true;
    Navigator.of(context).pop(true);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final request = widget.request;
    final body = request.body;
    final label = request.confirmLabel;
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 8, 24, 16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          Semantics(
            header: true,
            child: Text(
              request.title,
              style: theme.textTheme.headlineSmall?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          if (body != null && body.isNotEmpty)
            Text(
              body,
              style: theme.textTheme.bodyLarge?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          if (expiresMs != null)
            expired
                ? Semantics(
                    liveRegion: true,
                    child: Row(
                      spacing: 8,
                      children: [
                        Icon(
                          Icons.timer_off_outlined,
                          color: theme.colorScheme.error,
                        ),
                        Expanded(
                          child: Text(
                            widget.labels.expired,
                            style: theme.textTheme.bodyMedium?.copyWith(
                              color: theme.colorScheme.error,
                            ),
                          ),
                        ),
                      ],
                    ),
                  )
                : ExcludeSemantics(
                    child: AnimatedBuilder(
                      animation: countdown,
                      builder: (context, child) => LinearProgressIndicator(
                        value: countdown.value,
                        minHeight: 4,
                        borderRadius: BorderRadius.circular(2),
                      ),
                    ),
                  ),
          const SizedBox(height: 4),
          FilledButton(
            key: const ValueKey('polyui-confirm'),
            onPressed: armed && !expired ? confirm : null,
            style: FilledButton.styleFrom(
              minimumSize: const Size.fromHeight(50),
            ),
            child: Text(
              label == null || label.isEmpty
                  ? widget.labels.confirmFallback
                  : label,
            ),
          ),
          OutlinedButton(
            key: const ValueKey('polyui-cancel'),
            onPressed: () => Navigator.of(context).pop(false),
            style: OutlinedButton.styleFrom(
              minimumSize: const Size.fromHeight(50),
            ),
            child: Text(widget.labels.cancel),
          ),
        ],
      ),
    );
  }
}
