import 'dart:io';

import 'package:code_assets/code_assets.dart';
import 'package:crypto/crypto.dart';
import 'package:hooks/hooks.dart';

import 'prebuilt.dart';

const libraryName = 'polyui_ffi';
const assetName = 'polyui_ffi';
const cargoFeatures = 'lite';

void main(List<String> arguments) async {
  await build(arguments, (input, output) async {
    if (!input.config.buildCodeAssets) {
      return;
    }
    final code = input.config.code;
    final target = RustTarget.of(code);
    final bundled = input.outputDirectory.resolve(
      code.targetOS.dylibFileName(libraryName),
    );
    final workspace = rustWorkspace(input);
    if (workspace == null) {
      await downloadPrebuilt(input, target, bundled);
    } else {
      final cargo = findCargo();
      await ensureRustTarget(cargo, target.triple);
      final built = await runCargo(
        cargo: cargo,
        workspace: workspace,
        target: target,
        code: code,
      );
      await File.fromUri(built).copy(bundled.toFilePath());
      output.dependencies.addAll(rustSources(workspace));
    }

    output.assets.code.add(
      CodeAsset(
        package: input.packageName,
        name: assetName,
        linkMode: DynamicLoadingBundled(),
        file: bundled,
      ),
    );
  });
}

Uri? rustWorkspace(BuildInput input) {
  final configured = input.userDefines.path('rust_workspace');
  final candidate = configured ?? input.packageRoot.resolve('../../');
  final directory = candidate.path.endsWith('/')
      ? candidate
      : Uri.directory(candidate.toFilePath());
  final manifest = File.fromUri(directory.resolve('bindings/ffi/Cargo.toml'));
  if (manifest.existsSync()) {
    return directory;
  }
  if (configured != null) {
    throw StateError(
      'polyui could not find the Rust workspace at ${directory.toFilePath()}. '
      'hooks.user_defines.polyui.rust_workspace must point to the folder '
      'that holds bindings/ffi.',
    );
  }
  return null;
}

String prebuiltFileName(RustTarget target, OS os) =>
    os.dylibFileName('$libraryName-${target.triple}');

Uri prebuiltBase(BuildInput input) {
  final configured = input.userDefines['prebuilt_url'];
  if (configured == null) {
    return Uri.parse(
      'https://github.com/murtazox04/polyui/releases/download/'
      'v$prebuiltVersion/',
    );
  }
  final text = configured is String ? configured.trim() : '';
  final base = Uri.tryParse(text.endsWith('/') ? text : '$text/');
  if (base == null || (base.scheme != 'https' && base.scheme != 'file')) {
    throw StateError(
      'hooks.user_defines.polyui.prebuilt_url must be an https or file URL '
      'of a folder that holds the libpolyui_ffi files of a PolyUI release.',
    );
  }
  return base;
}

Future<void> downloadPrebuilt(
  BuildInput input,
  RustTarget target,
  Uri bundled,
) async {
  final fileName = prebuiltFileName(target, input.config.code.targetOS);
  final expected = prebuiltHashes[fileName];
  if (expected == null) {
    throw StateError(
      'polyui $prebuiltVersion has no prebuilt library for ${target.triple}. '
      'Build it from source by cloning https://github.com/murtazox04/polyui '
      'and setting hooks.user_defines.polyui.rust_workspace in your app '
      'pubspec to that folder.',
    );
  }
  final file = File.fromUri(bundled);
  if (file.existsSync() &&
      sha256.convert(await file.readAsBytes()).toString() == expected) {
    return;
  }
  final uri = prebuiltBase(input).resolve(fileName);
  final bytes = await fetch(uri);
  final actual = sha256.convert(bytes).toString();
  if (actual != expected) {
    throw StateError(
      'polyui fetched $uri but its SHA-256 is $actual, '
      'expected $expected. The file was not used.',
    );
  }
  await file.writeAsBytes(bytes, flush: true);
}

Future<List<int>> fetch(Uri uri) async {
  if (uri.scheme == 'file') {
    return File.fromUri(uri).readAsBytes();
  }
  final client = HttpClient()..findProxy = HttpClient.findProxyFromEnvironment;
  try {
    final response = await (await client.getUrl(uri)).close();
    if (response.statusCode != HttpStatus.ok) {
      throw HttpException(
        'polyui could not download $uri (HTTP ${response.statusCode})',
        uri: uri,
      );
    }
    final bytes = <int>[];
    await response.forEach(bytes.addAll);
    return bytes;
  } finally {
    client.close(force: true);
  }
}

String findCargo() {
  final path = Platform.environment['PATH'] ?? '';
  final separator = Platform.isWindows ? ';' : ':';
  final executable = Platform.isWindows ? 'cargo.exe' : 'cargo';
  for (final folder in path.split(separator)) {
    if (folder.isEmpty) {
      continue;
    }
    final candidate = File('$folder${Platform.pathSeparator}$executable');
    if (candidate.existsSync()) {
      return candidate.path;
    }
  }
  final home = Platform.environment['HOME'] ?? '';
  final fallback = File('$home/.cargo/bin/$executable');
  if (home.isNotEmpty && fallback.existsSync()) {
    return fallback.path;
  }
  throw StateError(
    'polyui needs cargo to build the PolyUI core. Install Rust with rustup '
    '(https://rustup.rs) and make sure cargo is on PATH.',
  );
}

Future<void> ensureRustTarget(String cargo, String triple) async {
  final rustup = File(
    '${File(cargo).parent.path}${Platform.pathSeparator}'
    '${Platform.isWindows ? 'rustup.exe' : 'rustup'}',
  );
  if (!rustup.existsSync()) {
    return;
  }
  final installed = await Process.run(rustup.path, [
    'target',
    'list',
    '--installed',
  ]);
  if (installed.exitCode != 0) {
    return;
  }
  final targets = '${installed.stdout}'.split('\n').map((line) => line.trim());
  if (targets.contains(triple)) {
    return;
  }
  final added = await Process.run(rustup.path, ['target', 'add', triple]);
  if (added.exitCode != 0) {
    throw ProcessException(
      rustup.path,
      ['target', 'add', triple],
      'rustup could not add $triple:\n${added.stderr}',
      added.exitCode,
    );
  }
}

final class RustTarget {
  const RustTarget(this.triple, {this.androidAbi});

  final String triple;
  final String? androidAbi;

  static RustTarget of(CodeConfig code) {
    final architecture = code.targetArchitecture;
    final os = code.targetOS;
    if (os == OS.macOS) {
      return switch (architecture) {
        Architecture.arm64 => const RustTarget('aarch64-apple-darwin'),
        Architecture.x64 => const RustTarget('x86_64-apple-darwin'),
        _ => unsupported(os, architecture),
      };
    }
    if (os == OS.iOS) {
      final simulator = code.iOS.targetSdk == IOSSdk.iPhoneSimulator;
      return switch (architecture) {
        Architecture.arm64 when simulator => const RustTarget(
          'aarch64-apple-ios-sim',
        ),
        Architecture.arm64 => const RustTarget('aarch64-apple-ios'),
        Architecture.x64 when simulator => const RustTarget('x86_64-apple-ios'),
        _ => unsupported(os, architecture),
      };
    }
    if (os == OS.android) {
      return switch (architecture) {
        Architecture.arm64 => const RustTarget(
          'aarch64-linux-android',
          androidAbi: 'arm64-v8a',
        ),
        Architecture.arm => const RustTarget(
          'armv7-linux-androideabi',
          androidAbi: 'armeabi-v7a',
        ),
        Architecture.x64 => const RustTarget(
          'x86_64-linux-android',
          androidAbi: 'x86_64',
        ),
        Architecture.ia32 => const RustTarget(
          'i686-linux-android',
          androidAbi: 'x86',
        ),
        _ => unsupported(os, architecture),
      };
    }
    if (os == OS.linux) {
      return switch (architecture) {
        Architecture.arm64 => const RustTarget('aarch64-unknown-linux-gnu'),
        Architecture.x64 => const RustTarget('x86_64-unknown-linux-gnu'),
        _ => unsupported(os, architecture),
      };
    }
    return unsupported(os, architecture);
  }

  static Never unsupported(OS os, Architecture architecture) {
    throw UnsupportedError(
      'polyui has no Rust target for $os on $architecture',
    );
  }
}

Future<Uri> runCargo({
  required String cargo,
  required Uri workspace,
  required RustTarget target,
  required CodeConfig code,
}) async {
  final manifest = workspace.resolve('Cargo.toml').toFilePath();
  final environment = <String, String>{
    'CARGO_PROFILE_RELEASE_STRIP': 'debuginfo',
  };
  final List<String> arguments;
  if (code.targetOS == OS.android) {
    final ndk = androidNdk(code);
    if (ndk != null) {
      environment['ANDROID_NDK_HOME'] = ndk;
    }
    arguments = [
      'ndk',
      '--target',
      target.androidAbi!,
      '--platform',
      '${code.android.targetNdkApi}',
      'build',
      '--release',
      '--lib',
      '--package',
      'polyui-ffi',
      '--features',
      cargoFeatures,
      '--manifest-path',
      manifest,
    ];
  } else {
    if (code.targetOS == OS.iOS) {
      environment['IPHONEOS_DEPLOYMENT_TARGET'] = '${code.iOS.targetVersion}.0';
    }
    if (code.targetOS == OS.macOS) {
      environment['MACOSX_DEPLOYMENT_TARGET'] = '${code.macOS.targetVersion}.0';
    }
    arguments = [
      'build',
      '--release',
      '--lib',
      '--package',
      'polyui-ffi',
      '--features',
      cargoFeatures,
      '--target',
      target.triple,
      '--manifest-path',
      manifest,
    ];
  }

  final result = await Process.run(
    cargo,
    arguments,
    workingDirectory: workspace.toFilePath(),
    environment: environment,
  );
  if (result.exitCode != 0 &&
      code.targetOS == OS.android &&
      '${result.stderr}'.contains('no such command')) {
    throw StateError(
      'polyui builds the Android library with cargo-ndk. '
      'Install it with: cargo install cargo-ndk',
    );
  }
  if (result.exitCode != 0) {
    throw ProcessException(
      cargo,
      arguments,
      'cargo failed for ${target.triple}:\n${result.stdout}\n${result.stderr}',
      result.exitCode,
    );
  }
  final built = workspace.resolve(
    'target/${target.triple}/release/${code.targetOS.dylibFileName(libraryName)}',
  );
  if (!File.fromUri(built).existsSync()) {
    throw StateError('cargo finished but ${built.toFilePath()} is missing');
  }
  return built;
}

String? androidNdk(CodeConfig code) {
  final compiler = code.cCompiler?.compiler;
  if (compiler == null) {
    return null;
  }
  final segments = compiler.pathSegments;
  final toolchains = segments.indexOf('toolchains');
  if (toolchains <= 0) {
    return null;
  }
  return Uri(
    scheme: 'file',
    pathSegments: [...segments.take(toolchains), ''],
  ).toFilePath();
}

List<Uri> rustSources(Uri workspace) {
  final sources = <Uri>[
    workspace.resolve('Cargo.toml'),
    workspace.resolve('Cargo.lock'),
    workspace.resolve('rust-toolchain.toml'),
  ];
  for (final folder in ['bindings/ffi', 'crates/polyui-core']) {
    final directory = Directory.fromUri(workspace.resolve('$folder/'));
    if (!directory.existsSync()) {
      continue;
    }
    for (final entity in directory.listSync(recursive: true)) {
      final path = entity.path;
      if (entity is File &&
          (path.endsWith('.rs') ||
              path.endsWith('.toml') ||
              path.endsWith('.json') ||
              path.endsWith('.html'))) {
        sources.add(entity.uri);
      }
    }
  }
  return sources.where((uri) => File.fromUri(uri).existsSync()).toList();
}
