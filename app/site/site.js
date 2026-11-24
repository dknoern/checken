// Checken's website: points the big Download buttons at the visitor's system
// and fills in the version and sizes of the latest build (downloads.json,
// written by the GitHub build when it publishes the site).
(function () {
  'use strict';

  const year = document.getElementById('year');
  if (year) year.textContent = String(new Date().getFullYear());

  // Which system is this visitor on?
  const ua = navigator.userAgent || '';
  const platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
  const mobile = /Android|iPhone|iPad|iPod/i.test(ua);
  let os = null;
  if (!mobile) {
    if (/Mac/i.test(platform) || /Macintosh/i.test(ua)) os = 'mac';
    else if (/Win/i.test(platform) || /Windows/i.test(ua)) os = 'windows';
    else if (/Linux|X11/i.test(platform + ua)) os = 'linux';
  }
  const names = { mac: 'macOS', windows: 'Windows', linux: 'Linux' };
  const files = {
    mac: 'downloads/Checken-mac.dmg',
    windows: 'downloads/Checken-Setup-windows.exe',
    linux: 'downloads/checken-linux-amd64.deb',
  };

  if (os) {
    for (const id of ['hero-download', 'cta-download']) {
      const b = document.getElementById(id);
      if (b) {
        b.textContent = 'Download for ' + names[os];
        b.href = files[os];
      }
    }
    const card = document.querySelector('.dl[data-os="' + os + '"]');
    if (card) card.classList.add('suggested');
  }

  const mb = (bytes) => (bytes / 1048576).toFixed(1) + ' MB';

  fetch('downloads.json', { cache: 'no-store' })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((info) => {
      const built = info.built ? new Date(info.built) : null;
      const when = built && !isNaN(built) ? built.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' }) : '';
      const version = info.version ? 'Version ' + info.version : 'Latest build';
      const meta = document.getElementById('hero-meta');
      if (meta) meta.textContent = version + (when ? ' · ' + when : '') + ' · macOS, Windows and Linux';
      const dl = document.getElementById('dl-version');
      if (dl) {
        dl.textContent = version + (when ? ', built ' + when : '') + (info.commit ? ' from commit ' + info.commit.slice(0, 7) : '') + '.';
      }
      for (const [key, f] of Object.entries(info.files || {})) {
        const el = document.querySelector('[data-meta="' + key + '"]');
        if (el && f.size) el.textContent = mb(f.size) + (f.sha256 ? ' · SHA-256 ' + f.sha256.slice(0, 16) + '…' : '');
        if (el && f.sha256) el.title = 'SHA-256 ' + f.sha256;
      }
      // A build that's missing a file: say so instead of offering a broken link.
      for (const a of document.querySelectorAll('[data-file]')) {
        if (!(info.files || {})[a.dataset.file]) {
          a.classList.add('missing');
          a.removeAttribute('href');
          a.textContent = 'Not in this build yet';
        }
      }
    })
    .catch(() => {
      // Opened from disk, or before the first published build: links stay as they are.
    });
})();
