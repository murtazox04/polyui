import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';

import '../core/json.dart';
import '../model/component_node.dart';
import 'core_components.dart';
import 'scope.dart';
import 'theme.dart';

@immutable
class ChoiceOption {
  const ChoiceOption(this.id, this.label, this.detail);

  final String id;
  final String label;
  final String detail;

  static List<ChoiceOption> read(Object? value) {
    final seen = <String>{};
    return [
      for (final item in jsonList(value))
        if (jsonString(asJsonMap(item)['id']) case final id? when seen.add(id))
          ChoiceOption(
            id,
            jsonString(asJsonMap(item)['label']) ?? '',
            jsonString(asJsonMap(item)['detail']) ?? '',
          ),
    ];
  }

  static Set<String> selected(Object? value) => switch (value) {
    final String id => {id},
    final List<Object?> items => items.whereType<String>().toSet(),
    _ => const {},
  };
}

class ChoiceGroupComponent extends StatelessWidget {
  const ChoiceGroupComponent({super.key, required this.node});

  final ComponentNode node;

  void toggle(
    PolyuiScope scope,
    ChoiceOption option,
    List<ChoiceOption> options,
    Set<String> selected,
    bool multiple,
  ) {
    final path = jsonString(node.props['value']);
    if (path == null) {
      return;
    }
    final Object next;
    if (multiple) {
      final chosen = {...selected};
      if (!chosen.remove(option.id)) {
        chosen.add(option.id);
      }
      next = [
        for (final item in options)
          if (chosen.contains(item.id)) item.id,
      ];
    } else {
      next = option.id;
    }
    scope.session.setValue(surface: scope.surface.id, path: path, value: next);
    if (node.hasAction('on_change')) {
      scope.session.fire(
        surface: scope.surface.id,
        component: node.id,
        prop: 'on_change',
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final scope = PolyuiScope.of(context);
    final options = ChoiceOption.read(node.props['options']);
    final selected = ChoiceOption.selected(node.value);
    final multiple = node.flag('multiple');
    final label = node.string('label');
    final radius = BorderRadius.circular(theme.innerCornerRadius);
    return FillWidth(
      child: ContainerSemantics(
        node: node,
        role: multiple ? null : SemanticsRole.radioGroup,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          spacing: 8,
          children: [
            if (label.isNotEmpty)
              ExcludeSemantics(
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 15,
                    fontWeight: FontWeight.w600,
                    color: theme.muted,
                  ),
                ),
              ),
            DecoratedBox(
              position: DecorationPosition.foreground,
              decoration: BoxDecoration(
                borderRadius: radius,
                border: Border.all(color: theme.border),
              ),
              child: ClipRRect(
                borderRadius: radius,
                child: ColoredBox(
                  color: theme.background,
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      for (final (index, option) in options.indexed) ...[
                        if (index > 0)
                          Container(height: 1, color: theme.border),
                        ChoiceRow(
                          key: ValueKey(option.id),
                          option: option,
                          on: selected.contains(option.id),
                          multiple: multiple,
                          onTap: () => toggle(
                            scope,
                            option,
                            options,
                            selected,
                            multiple,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class ChoiceRow extends StatelessWidget {
  const ChoiceRow({
    super.key,
    required this.option,
    required this.on,
    required this.multiple,
    required this.onTap,
  });

  final ChoiceOption option;
  final bool on;
  final bool multiple;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final icon = multiple
        ? (on ? Icons.check_box : Icons.check_box_outline_blank)
        : (on ? Icons.radio_button_checked : Icons.radio_button_unchecked);
    return Semantics(
      container: true,
      checked: on,
      inMutuallyExclusiveGroup: multiple ? null : true,
      label: option.detail.isEmpty
          ? option.label
          : '${option.label}, ${option.detail}',
      onTap: onTap,
      excludeSemantics: true,
      child: Material(
        type: MaterialType.transparency,
        child: InkWell(
          onTap: onTap,
          splashFactory: reducedMotion(context) ? NoSplash.splashFactory : null,
          child: AnimatedContainer(
            duration: motion(context, 150),
            curve: Curves.easeOut,
            constraints: const BoxConstraints(minHeight: 44),
            color: on
                ? theme.accent.withValues(alpha: 0.08)
                : Colors.transparent,
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
            child: Row(
              spacing: 12,
              children: [
                Icon(
                  icon,
                  size: 24,
                  color: on ? theme.accent : theme.borderStrong,
                ),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    spacing: 2,
                    children: [
                      Text(
                        option.label,
                        style: TextStyle(fontSize: 17, color: theme.foreground),
                      ),
                      if (option.detail.isNotEmpty)
                        Text(
                          option.detail,
                          style: TextStyle(fontSize: 13, color: theme.muted),
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class InputComponent extends StatefulWidget {
  const InputComponent({super.key, required this.node});

  final ComponentNode node;

  @override
  State<InputComponent> createState() => InputComponentState();
}

class InputComponentState extends State<InputComponent> {
  static const recentLimit = 8;

  final TextEditingController controller = TextEditingController();
  final FocusNode focusNode = FocusNode();
  final List<String> recent = [];

  ComponentNode get node => widget.node;

  int get maxLength {
    final raw = node.number('max_length');
    return raw == null || raw < 1 ? 200 : raw.toInt();
  }

  static String display(Object? value) => switch (value) {
    final String text => text,
    final int number => '$number',
    final double number when number.isFinite =>
      number == number.roundToDouble() && number.abs() < 1e15
          ? '${number.toInt()}'
          : '$number',
    _ => '',
  };

  @override
  void initState() {
    super.initState();
    controller.text = display(node.value);
    node.addListener(sync);
    focusNode.addListener(focusChanged);
  }

  @override
  void didUpdateWidget(InputComponent oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.node != node) {
      oldWidget.node.removeListener(sync);
      node.addListener(sync);
      sync();
    }
  }

  @override
  void dispose() {
    node.removeListener(sync);
    focusNode.removeListener(focusChanged);
    focusNode.dispose();
    controller.dispose();
    super.dispose();
  }

  void focusChanged() {
    if (!focusNode.hasFocus) {
      recent.clear();
    }
    setState(() {});
  }

  void sync() {
    final desired = display(node.value);
    if (desired == controller.text) {
      return;
    }
    if (focusNode.hasFocus && recent.contains(desired)) {
      return;
    }
    final caret = controller.selection.isValid
        ? controller.selection.extentOffset.clamp(0, desired.length)
        : desired.length;
    controller.value = TextEditingValue(
      text: desired,
      selection: TextSelection.collapsed(
        offset: focusNode.hasFocus ? caret : desired.length,
      ),
    );
  }

  void edited(String typed) {
    final path = jsonString(node.props['value']);
    if (path == null || typed == display(node.value)) {
      return;
    }
    recent.add(typed);
    if (recent.length > recentLimit) {
      recent.removeAt(0);
    }
    final scope = PolyuiScope.of(context);
    scope.session.setValue(surface: scope.surface.id, path: path, value: typed);
  }

  @override
  Widget build(BuildContext context) {
    final theme = PolyuiTheme.of(context);
    final kind = node.choice('input', const {
      'text',
      'number',
      'phone',
      'email',
      'multiline',
    }, 'text');
    final label = node.string('label');
    final placeholder = node.string('placeholder');
    final focused = focusNode.hasFocus;
    OutlineInputBorder outline(Color color, double width) => OutlineInputBorder(
      borderRadius: BorderRadius.circular(theme.controlCornerRadius),
      borderSide: BorderSide(color: color, width: width),
    );
    return FillWidth(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        spacing: 6,
        children: [
          ExcludeSemantics(
            child: Text(
              label,
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w500,
                color: theme.foreground,
              ),
            ),
          ),
          Semantics(
            label: node.a11y.label.isEmpty ? label : node.a11y.label,
            child: TextField(
              controller: controller,
              focusNode: focusNode,
              onChanged: edited,
              keyboardType: switch (kind) {
                'number' => const TextInputType.numberWithOptions(
                  decimal: true,
                  signed: true,
                ),
                'phone' => TextInputType.phone,
                'email' => TextInputType.emailAddress,
                'multiline' => TextInputType.multiline,
                _ => TextInputType.text,
              },
              autofillHints: switch (kind) {
                'phone' => const [AutofillHints.telephoneNumber],
                'email' => const [AutofillHints.email],
                _ => null,
              },
              autocorrect: kind == 'text' || kind == 'multiline',
              textCapitalization: kind == 'text' || kind == 'multiline'
                  ? TextCapitalization.sentences
                  : TextCapitalization.none,
              minLines: kind == 'multiline' ? 3 : 1,
              maxLines: kind == 'multiline' ? 8 : 1,
              inputFormatters: [LengthLimitingTextInputFormatter(maxLength)],
              cursorColor: theme.accent,
              style: TextStyle(fontSize: 17, color: theme.foreground),
              decoration: InputDecoration(
                hintText: placeholder.isEmpty ? null : placeholder,
                hintStyle: TextStyle(fontSize: 17, color: theme.muted),
                isDense: true,
                filled: true,
                fillColor: theme.background,
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 12,
                ),
                enabledBorder: outline(theme.borderStrong, 1),
                focusedBorder: outline(theme.accent, 1.5),
                border: outline(focused ? theme.accent : theme.borderStrong, 1),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
