# Flux 2.4.3 validation

The release workflows exercise the Windows installer/portable application, installed Fedora 43 RPM, shared browser runtime and core regressions. The published source archive contains the workflow-generated `docs/validation/release.json`; `release-info.json` records the exact source commit and successful platform runs. Historical 2.4.2 reports are not substituted for 2.4.3 results.

For Chromium, installer tests run separate isolated profiles with the GitHub identity `iheejmamcoeigoallciifjminjnnnlbj` and the store identity `okimieponfbegjhmfdnnefnddofooipk`. The store test uses the store's public key in a temporary unpacked copy to reproduce its identity, without changing the distributed GitHub key or the keyless store ZIP. Both paths exercise file handoff, native approval and acknowledged video handoff. This tests identity and runtime compatibility; it does not constitute Google approval or installation of a Google-signed store package.

The installed Linux RPM must register exactly these two Chromium origins, and the installed native host is queried through each origin. The existing Firefox identity remains unchanged. Store archive verification permits only the intended manifest identity/update-field differences and compares every other runtime file against the shared source. Nineteen locale titles and summaries are included in both distributions.

## Türkçe

Yayın iş akışları Windows kurulumunu/taşınabilir uygulamayı, Fedora 43'e kurulmuş RPM'i, ortak eklenti kodunu ve mevcut çekirdek regresyonlarını sınar. Kaynak arşivindeki `docs/validation/release.json` test sırasında üretilir; `release-info.json` tam kaynak commit'ini ve başarılı platform çalışmalarını belirtir. Eski 2.4.2 raporları bu sürümün sonucu yerine kullanılmaz.

Windows testinde GitHub ve Chrome mağazası kimlikleri ayrı Chromium profillerinde denenir. Mağaza kimliği, Google'ın açık anahtarıyla geçici bir açılmış eklenti kopyasında oluşturulur; GitHub paketinin anahtarı ve mağaza ZIP'inin anahtarsız yapısı korunur. İki dağıtım için dosya aktarımı, yerel onay ve onaylı medya aktarımı kontrol edilir. Bu test Google onayı veya Google imzalı mağaza paketinin kurulumu anlamına gelmez.

Linux'ta kurulu RPM iki Chromium kimliğini de kaydetmeli ve kurulu yerel köprü ikisinden de isteği kabul etmelidir. Firefox kimliği korunur. Mağaza arşivleri sadece izin verilen manifest farklarıyla ortak kaynaklara karşı doğrulanır.
