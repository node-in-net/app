const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
if (args.length < 2) {
	console.error('Usage: node gen-version.js <app_type> <build_type>');
	console.error('Example: node gen-version.js gui deb');
	process.exit(1);
}

const app_type = args[0];
const build_type = args[1];

const packageJsonPath = path.join(__dirname, '..', 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
const version = packageJson.version;

const outPath = path.join(
	__dirname,
	'..',
	'src',
	'app-version',
	'src',
	'version.rs',
);

const content = `pub const APP_VERSION: &str = "${version}";
pub const APP_TYPE: &str = "${app_type}";
pub const BUILD_TYPE: &str = "${build_type}";
`;

fs.writeFileSync(outPath, content, 'utf8');

console.log(`Generated version.rs -> v${version} [${app_type}:${build_type}]`);

function replaceSectionVersion(content, sectionHeader, newVersion) {
	const lines = content.split('\n');
	let inSection = false;
	let modified = false;

	for (let i = 0; i < lines.length; i++) {
		const trimmed = lines[i].trim();
		if (trimmed.startsWith('[')) {
			inSection = trimmed === sectionHeader;
			continue;
		}
		if (inSection && /^version\s*=/.test(trimmed)) {
			lines[i] = lines[i].replace(/=.*$/, `= "${newVersion}"`);
			modified = true;
			inSection = false;
		}
	}
	return { content: lines.join('\n'), modified };
}

function syncCrateVersions() {
	const srcDir = path.join(__dirname, '..', 'src');
	for (const entry of fs.readdirSync(srcDir)) {
		const manifest = path.join(srcDir, entry, 'Cargo.toml');
		if (!fs.existsSync(manifest)) continue;

		let content = fs.readFileSync(manifest, 'utf8');
		let touched = false;
		for (const section of ['[package]', '[package.metadata.bundle]']) {
			const res = replaceSectionVersion(content, section, version);
			content = res.content;
			touched = touched || res.modified;
		}
		if (touched) {
			fs.writeFileSync(manifest, content, 'utf8');
			console.log(`Auto-synced ${entry}/Cargo.toml -> v${version}`);
		}
	}
}


function updateSetupNsi(nsiPath) {
	if (!fs.existsSync(nsiPath)) return;

	let content = fs.readFileSync(nsiPath, 'utf8');
	let modified = false;

	const regStrRegex =
		/(WriteRegStr\s+HKLM\s+"[^"]+"\s+"DisplayVersion"\s+)"[^"]+"/g;
	if (regStrRegex.test(content)) {
		content = content.replace(regStrRegex, `$1"${version}"`);
		modified = true;
	}

	const outFileRegex = /^(OutFile\s+)"[^"]+"/m;
	if (outFileRegex.test(content)) {
		content = content.replace(
			outFileRegex,
			`$1"..\\\\..\\\\distr\\\\nodeinnet-gtk-${version}-1-win64.exe"`,
		);
		modified = true;
	}

	if (modified) {
		fs.writeFileSync(nsiPath, content, 'utf8');
		console.log(`Auto-synced OutFile and DisplayVersion in setup.nsi -> v${version}`);
	}
}

function updateBuildGradle(gradlePath) {
	if (fs.existsSync(gradlePath)) {
		let content = fs.readFileSync(gradlePath, 'utf8');
		let modified = false;

		const vNameRegex = /versionName\s*=\s*"[^"]+"/;
		if (vNameRegex.test(content)) {
			content = content.replace(vNameRegex, `versionName = "${version}"`);
			modified = true;
		}

		const vCodeRegex = /versionCode\s*=\s*\d+/;
		if (vCodeRegex.test(content)) {
			const vParts = version.split('.');
			let code = 1;
			if (vParts.length >= 3) {
				code =
					parseInt(vParts[0]) * 1000000 +
					parseInt(vParts[1]) * 1000 +
					parseInt(vParts[2]);
			}
			content = content.replace(vCodeRegex, `versionCode = ${code}`);
			modified = true;
		}

		if (modified) {
			fs.writeFileSync(gradlePath, content, 'utf8');
			console.log(`Auto-synced Android build.gradle.kts -> v${version}`);
		}
	}
}

updateSetupNsi(path.join(__dirname, '..', 'src', 'gtk-app', 'setup.nsi'));

updateBuildGradle(
	path.join(__dirname, '..', 'src', 'android-app', 'app', 'build.gradle.kts'),
);

syncCrateVersions();
