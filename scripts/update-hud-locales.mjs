import { readFile, writeFile, readdir } from "node:fs/promises";

const translations = {
  "en-US": "Choose the video quality and output format, then send it to Flux.",
  "tr-TR": "Video kalitesini ve çıktı biçimini seçip Flux'a gönderin.",
  "ar-SA": "اختر جودة الفيديو وتنسيق الإخراج، ثم أرسله إلى Flux.",
  "bn-BD": "ভিডিওর মান ও আউটপুট ফরম্যাট বেছে নিয়ে Flux-এ পাঠান।",
  "de-DE": "Wähle Videoqualität und Ausgabeformat und sende das Video an Flux.",
  "es-ES":
    "Elige la calidad del vídeo y el formato de salida y envíalo a Flux.",
  "fr-FR":
    "Choisissez la qualité vidéo et le format de sortie, puis envoyez la vidéo à Flux.",
  "hi-IN":
    "वीडियो की गुणवत्ता और आउटपुट फ़ॉर्मैट चुनें, फिर उसे Flux में भेजें।",
  "id-ID": "Pilih kualitas video dan format keluaran, lalu kirim ke Flux.",
  "ja-JP": "画質と出力形式を選択して、Flux に送信してください。",
  "mr-IN": "व्हिडिओची गुणवत्ता आणि आउटपुट स्वरूप निवडून Flux कडे पाठवा.",
  "pcm-NG": "Choose video quality and output format, then send am to Flux.",
  "pt-BR":
    "Escolha a qualidade do vídeo e o formato de saída e envie para o Flux.",
  "ru-RU": "Выберите качество видео и выходной формат, затем отправьте в Flux.",
  "ta-IN":
    "வீடியோ தரத்தையும் வெளியீட்டு வடிவத்தையும் தேர்ந்தெடுத்து Flux-க்கு அனுப்பவும்.",
  "te-IN": "వీడియో నాణ్యతను, అవుట్‌పుట్ ఫార్మాట్‌ను ఎంచుకుని Flux కు పంపండి.",
  "ur-PK": "ویڈیو کا معیار اور آؤٹ پٹ فارمیٹ منتخب کرکے Flux کو بھیجیں۔",
  "vi-VN": "Chọn chất lượng video và định dạng đầu ra, rồi gửi đến Flux.",
  "yue-HK": "揀選影片畫質同輸出格式，再傳送到 Flux。",
  "zh-CN": "选择视频画质和输出格式，然后发送到 Flux。",
};

for (const [locale, message] of Object.entries(translations)) {
  for (const directory of ["src/locales", "extension/locales"]) {
    const file = `${directory}/${locale}.json`;
    const data = JSON.parse(await readFile(file, "utf8"));
    data.mediaReady = message;
    await writeFile(file, JSON.stringify(data, null, 2) + "\n");
  }
}
for (const directory of await readdir("extension/_locales")) {
  const file = `extension/_locales/${directory}/messages.json`;
  const data = JSON.parse(await readFile(file, "utf8"));
  const locale =
    directory === "zh_TW"
      ? "yue-HK"
      : Object.keys(translations).find(
          (locale) =>
            locale.replace("-", "_") === directory ||
            locale.split("-")[0] === directory,
        );
  if (!locale) throw Error(`Unmapped Chrome locale: ${directory}`);
  data.mediaReady = { message: translations[locale] };
  await writeFile(file, JSON.stringify(data, null, 2) + "\n");
}
