import 'dart:ui' show lerpDouble;

import 'package:flutter/material.dart';

@immutable
class PolyuiThemeData extends ThemeExtension<PolyuiThemeData> {
  const PolyuiThemeData({
    required this.background,
    required this.sunken,
    required this.raised,
    required this.foreground,
    required this.muted,
    required this.border,
    required this.borderStrong,
    required this.accent,
    required this.accentForeground,
    required this.positive,
    required this.negative,
    required this.warning,
    required this.info,
    required this.series,
    required this.chartGrid,
    required this.chartAxis,
    this.cornerRadius = 16,
    this.innerCornerRadius = 12,
    this.controlCornerRadius = 10,
    this.spacing = 16,
    this.cardShadow = true,
  });

  static const light = PolyuiThemeData(
    background: Color(0xFFFEFDFC),
    sunken: Color(0xFFF5F5F2),
    raised: Color(0xFFFFFEFD),
    foreground: Color(0xFF1A1D23),
    muted: Color(0xFF575B63),
    border: Color(0xFFDFE1E5),
    borderStrong: Color(0xFFC1C4C9),
    accent: Color(0xFF00686A),
    accentForeground: Color(0xFFF9FDFD),
    positive: Color(0xFF0A6D37),
    negative: Color(0xFFB02B27),
    warning: Color(0xFF995600),
    info: Color(0xFF1B62A7),
    series: [
      Color(0xFF007D7F),
      Color(0xFFE49839),
      Color(0xFF476DB4),
      Color(0xFFD45D66),
      Color(0xFF4E9A5B),
      Color(0xFF885CB5),
      Color(0xFF0087AB),
      Color(0xFF697284),
    ],
    chartGrid: Color(0xFFE5E6E9),
    chartAxis: Color(0xFFC1C4C9),
  );

  static const dark = PolyuiThemeData(
    background: Color(0xFF191B1E),
    sunken: Color(0xFF121316),
    raised: Color(0xFF222428),
    foreground: Color(0xFFF1F0ED),
    muted: Color(0xFFA1A5AB),
    border: Color(0xFF2D2F33),
    borderStrong: Color(0xFF484A50),
    accent: Color(0xFF5ED0C7),
    accentForeground: Color(0xFF011D1F),
    positive: Color(0xFF6AD18A),
    negative: Color(0xFFF87E77),
    warning: Color(0xFFF2B95A),
    info: Color(0xFF7FBEF3),
    series: [
      Color(0xFF49C7C0),
      Color(0xFFEEB154),
      Color(0xFF7FA4EA),
      Color(0xFFEB8186),
      Color(0xFF73C881),
      Color(0xFFBC97E8),
      Color(0xFF6DBDDC),
      Color(0xFF9BA5B8),
    ],
    chartGrid: Color(0xFF2A2B2E),
    chartAxis: Color(0xFF484A50),
    cardShadow: false,
  );

  final Color background;
  final Color sunken;
  final Color raised;
  final Color foreground;
  final Color muted;
  final Color border;
  final Color borderStrong;
  final Color accent;
  final Color accentForeground;
  final Color positive;
  final Color negative;
  final Color warning;
  final Color info;
  final List<Color> series;
  final Color chartGrid;
  final Color chartAxis;
  final double cornerRadius;
  final double innerCornerRadius;
  final double controlCornerRadius;
  final double spacing;
  final bool cardShadow;

  Color? chartColor(String? token) {
    switch (token) {
      case null:
        return null;
      case 'axis':
        return chartAxis;
      case 'grid':
        return chartGrid;
      case 'text':
        return foreground;
      case 'muted':
        return muted;
    }
    if (!token.startsWith('series-')) {
      return null;
    }
    final index = int.tryParse(token.substring(7));
    if (index == null || index < 1) {
      return null;
    }
    final palette = series.isEmpty ? [accent] : series;
    return palette[(index - 1) % palette.length];
  }

  Color tone(String name) => switch (name) {
    'info' => info,
    'success' || 'positive' => positive,
    'warning' => warning,
    'danger' || 'negative' => negative,
    _ => muted,
  };

  @override
  PolyuiThemeData copyWith({
    Color? background,
    Color? sunken,
    Color? raised,
    Color? foreground,
    Color? muted,
    Color? border,
    Color? borderStrong,
    Color? accent,
    Color? accentForeground,
    Color? positive,
    Color? negative,
    Color? warning,
    Color? info,
    List<Color>? series,
    Color? chartGrid,
    Color? chartAxis,
    double? cornerRadius,
    double? innerCornerRadius,
    double? controlCornerRadius,
    double? spacing,
    bool? cardShadow,
  }) => PolyuiThemeData(
    background: background ?? this.background,
    sunken: sunken ?? this.sunken,
    raised: raised ?? this.raised,
    foreground: foreground ?? this.foreground,
    muted: muted ?? this.muted,
    border: border ?? this.border,
    borderStrong: borderStrong ?? this.borderStrong,
    accent: accent ?? this.accent,
    accentForeground: accentForeground ?? this.accentForeground,
    positive: positive ?? this.positive,
    negative: negative ?? this.negative,
    warning: warning ?? this.warning,
    info: info ?? this.info,
    series: series ?? this.series,
    chartGrid: chartGrid ?? this.chartGrid,
    chartAxis: chartAxis ?? this.chartAxis,
    cornerRadius: cornerRadius ?? this.cornerRadius,
    innerCornerRadius: innerCornerRadius ?? this.innerCornerRadius,
    controlCornerRadius: controlCornerRadius ?? this.controlCornerRadius,
    spacing: spacing ?? this.spacing,
    cardShadow: cardShadow ?? this.cardShadow,
  );

  @override
  PolyuiThemeData lerp(PolyuiThemeData? other, double t) {
    if (other == null) {
      return this;
    }
    Color mix(Color from, Color to) => Color.lerp(from, to, t)!;
    double blend(double from, double to) => lerpDouble(from, to, t)!;
    final count = series.length > other.series.length
        ? series.length
        : other.series.length;
    return PolyuiThemeData(
      background: mix(background, other.background),
      sunken: mix(sunken, other.sunken),
      raised: mix(raised, other.raised),
      foreground: mix(foreground, other.foreground),
      muted: mix(muted, other.muted),
      border: mix(border, other.border),
      borderStrong: mix(borderStrong, other.borderStrong),
      accent: mix(accent, other.accent),
      accentForeground: mix(accentForeground, other.accentForeground),
      positive: mix(positive, other.positive),
      negative: mix(negative, other.negative),
      warning: mix(warning, other.warning),
      info: mix(info, other.info),
      series: [
        for (var index = 0; index < count; index += 1)
          mix(
            series.isEmpty ? accent : series[index % series.length],
            other.series.isEmpty
                ? other.accent
                : other.series[index % other.series.length],
          ),
      ],
      chartGrid: mix(chartGrid, other.chartGrid),
      chartAxis: mix(chartAxis, other.chartAxis),
      cornerRadius: blend(cornerRadius, other.cornerRadius),
      innerCornerRadius: blend(innerCornerRadius, other.innerCornerRadius),
      controlCornerRadius: blend(
        controlCornerRadius,
        other.controlCornerRadius,
      ),
      spacing: blend(spacing, other.spacing),
      cardShadow: t < 0.5 ? cardShadow : other.cardShadow,
    );
  }
}

class PolyuiTheme extends InheritedWidget {
  const PolyuiTheme({super.key, required this.data, required super.child});

  final PolyuiThemeData data;

  static PolyuiThemeData of(BuildContext context) {
    final scoped = context.dependOnInheritedWidgetOfExactType<PolyuiTheme>();
    if (scoped != null) {
      return scoped.data;
    }
    final theme = Theme.of(context);
    return theme.extension<PolyuiThemeData>() ??
        (theme.brightness == Brightness.dark
            ? PolyuiThemeData.dark
            : PolyuiThemeData.light);
  }

  @override
  bool updateShouldNotify(PolyuiTheme oldWidget) => oldWidget.data != data;
}

@immutable
class PolyuiLabels {
  const PolyuiLabels({
    this.cancel = 'Cancel',
    this.confirmFallback = 'OK',
    this.expired = 'This request expired. Ask again to continue.',
    this.chartUnavailable = 'Chart unavailable',
    this.openLinkTitle = 'Open this link?',
    this.openLinkConfirm = 'Open link',
  });

  final String cancel;
  final String confirmFallback;
  final String expired;
  final String chartUnavailable;
  final String openLinkTitle;
  final String openLinkConfirm;
}
