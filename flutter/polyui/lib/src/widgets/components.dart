import 'package:flutter/widgets.dart';

import '../model/component_node.dart';
import 'chart_components.dart';
import 'core_components.dart';
import 'embed_component.dart';
import 'form_components.dart';
import 'scope.dart';

Widget buildComponent(
  BuildContext context,
  PolyuiScope scope,
  ComponentNode node,
) {
  final custom = scope.builders[node.type];
  if (custom != null) {
    return custom(context, scope.contextFor(node));
  }
  return switch (node.type) {
    'Text' => TextComponent(node: node),
    'Card' => CardComponent(node: node),
    'Row' => RowComponent(node: node),
    'List' => ListComponent(node: node),
    'Stat' => StatComponent(node: node),
    'Badge' => BadgeComponent(node: node),
    'Image' => ImageComponent(node: node),
    'Button' => ButtonComponent(node: node),
    'ChoiceGroup' => ChoiceGroupComponent(node: node),
    'Input' => InputComponent(node: node),
    'ConfirmSheet' => ConfirmSheetComponent(node: node),
    'Divider' => const DividerComponent(),
    'chart:Chart' => ChartComponent(node: node),
    'chart:Dashboard' => DashboardComponent(node: node),
    'embed:Embed' => EmbedComponent(node: node),
    _ => UnknownComponent(node: node),
  };
}
