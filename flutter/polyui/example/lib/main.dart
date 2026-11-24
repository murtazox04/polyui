import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:polyui/polyui.dart';

import 'src/chat_model.dart';
import 'src/chat_screen.dart';
import 'src/demo_script.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final text = await rootBundle.loadString('assets/conversation.json');
  const demo = DemoScript();
  final model = ChatModel(
    script: jsonDecode(text) as Map<String, Object?>,
    instant: demo.instant,
  );
  runApp(PolyuiExampleApp(model: model, demo: demo));
}

ThemeData appTheme(Brightness brightness) {
  final polyui = brightness == Brightness.dark
      ? PolyuiThemeData.dark
      : PolyuiThemeData.light;
  final page = brightness == Brightness.dark
      ? const Color(0xFF111316)
      : const Color(0xFFF3F6F8);
  final base = ThemeData(brightness: brightness);
  TextStyle? untracked(TextStyle? style) => style?.copyWith(letterSpacing: 0);
  final text = base.textTheme;
  return ThemeData(
    brightness: brightness,
    textTheme: text.copyWith(
      displayLarge: untracked(text.displayLarge),
      displayMedium: untracked(text.displayMedium),
      displaySmall: untracked(text.displaySmall),
      headlineLarge: untracked(text.headlineLarge),
      headlineMedium: untracked(text.headlineMedium),
      headlineSmall: untracked(text.headlineSmall),
      titleLarge: untracked(text.titleLarge),
      titleMedium: untracked(text.titleMedium),
      titleSmall: untracked(text.titleSmall),
      bodyLarge: untracked(text.bodyLarge),
      bodyMedium: untracked(text.bodyMedium),
      bodySmall: untracked(text.bodySmall),
      labelLarge: untracked(text.labelLarge),
      labelMedium: untracked(text.labelMedium),
      labelSmall: untracked(text.labelSmall),
    ),
    colorScheme: ColorScheme.fromSeed(
      seedColor: polyui.accent,
      brightness: brightness,
      primary: polyui.accent,
      onPrimary: polyui.accentForeground,
    ),
    scaffoldBackgroundColor: page,
    extensions: [polyui],
  );
}

class PolyuiExampleApp extends StatelessWidget {
  const PolyuiExampleApp({super.key, required this.model, required this.demo});

  final ChatModel model;
  final DemoScript demo;

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'PolyUI',
    debugShowCheckedModeBanner: false,
    theme: appTheme(Brightness.light),
    darkTheme: appTheme(Brightness.dark),
    home: ChatScreen(model: model, demo: demo),
  );
}
