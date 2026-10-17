import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from "node:fs";
import { createHash, generateKeyPairSync } from "node:crypto";
const storeMetadata = JSON.parse(
  readFileSync(
    new URL("./browser-store-metadata.json", import.meta.url),
    "utf8",
  ),
);
const keys =
  "downloadMedia|downloadWith|captureLinks|quality|automatic|inspecting|sentToDesktop|retry|wildcard|downloadSelected|interceptDownloads|interceptExplanation|browserIntegration|nativeInstallHelp|saved|desktopOffline|waitingForDesktop|noMedia|mediaHint|batch|priority|start_at|stop_at|media_processing|audio_url|duration_seconds|container|concurrent_tasks|download_directory|choose_folder|queue_enabled|clipboard_monitor|minimize_to_tray|notifications|save|open_file|reveal_file|remove|remove_description|confirm|inspect|queue_start|pause_all|install_bridge|installed|copy_hash".split(
    "|",
  );
const catalogs = {
  "en-US":
    "Download media|Download with Flux|Capture page links|Quality / codec|Automatic|Inspecting media…|Saved to the desktop queue|Retry|Filter with *.zip|Download selected|Intercept browser downloads|The browser keeps downloading until Flux confirms receipt.|Browser integration|Open Flux desktop settings and install the browser bridge.|Saved|Desktop offline|Waiting for desktop|No media detected|Play a video to detect its media requests.|Batch download|Priority|Start at|Stop at|Process media with FFmpeg|Separate audio URL (optional)|Recording duration (seconds, optional)|Container|Concurrent downloads|Download folder|Choose folder|Run the queue|Monitor clipboard links|Minimize to tray|Completion notifications|Save settings|Open file|Show in folder|Remove task|Remove this task and its temporary data? The completed file is kept.|Remove|Inspect streams|Start queue|Pause all|Install browser bridge|Browser bridge installed|Copy SHA-256",
  "zh-CN":
    "下载媒体|使用Flux下载|获取页面链接|画质 / 编码|自动|正在分析媒体…|已保存至桌面队列|重试|使用*.zip筛选|下载所选|接管浏览器下载|Flux确认接收前，浏览器继续下载。|浏览器集成|打开Flux桌面设置并安装浏览器桥接。|已保存|桌面程序离线|等待桌面程序|未检测到媒体|播放视频以检测媒体请求。|批量下载|优先级|开始时间|停止时间|使用FFmpeg处理媒体|独立音频地址（可选）|录制时长（秒，可选）|封装格式|同时下载数|下载文件夹|选择文件夹|运行队列|监测剪贴板链接|最小化到托盘|完成通知|保存设置|打开文件|在文件夹中显示|移除任务|移除任务及临时数据？已完成文件将保留。|移除|分析媒体流|启动队列|全部暂停|安装浏览器桥接|浏览器桥接已安装|复制SHA-256",
  "hi-IN":
    "मीडिया डाउनलोड करें|Flux से डाउनलोड करें|पृष्ठ के लिंक पकड़ें|गुणवत्ता / कोडेक|स्वचालित|मीडिया जाँच हो रही है…|डेस्कटॉप कतार में सहेजा गया|फिर कोशिश करें|*.zip से छाँटें|चुने हुए डाउनलोड करें|ब्राउज़र डाउनलोड संभालें|Flux की पुष्टि तक ब्राउज़र डाउनलोड जारी रखता है।|ब्राउज़र एकीकरण|Flux डेस्कटॉप सेटिंग खोलकर ब्राउज़र ब्रिज स्थापित करें।|सहेजा गया|डेस्कटॉप ऑफ़लाइन|डेस्कटॉप की प्रतीक्षा|मीडिया नहीं मिला|मीडिया अनुरोध पहचानने के लिए वीडियो चलाएँ।|समूह डाउनलोड|प्राथमिकता|आरंभ समय|समाप्ति समय|FFmpeg से मीडिया संसाधित करें|अलग ऑडियो URL (वैकल्पिक)|रिकॉर्डिंग अवधि (सेकंड, वैकल्पिक)|कंटेनर|एक साथ डाउनलोड|डाउनलोड फ़ोल्डर|फ़ोल्डर चुनें|कतार चलाएँ|क्लिपबोर्ड लिंक देखें|ट्रे में छोटा करें|पूर्ण होने की सूचनाएँ|सेटिंग सहेजें|फ़ाइल खोलें|फ़ोल्डर में दिखाएँ|कार्य हटाएँ|कार्य और अस्थायी डेटा हटाएँ? पूर्ण फ़ाइल सुरक्षित रहेगी।|हटाएँ|स्ट्रीम जाँचें|कतार शुरू करें|सब रोकें|ब्राउज़र ब्रिज स्थापित करें|ब्राउज़र ब्रिज स्थापित हुआ|SHA-256 कॉपी करें",
  "es-ES":
    "Descargar contenido|Descargar con Flux|Capturar enlaces de la página|Calidad / códec|Automático|Analizando contenido…|Guardado en la cola de escritorio|Reintentar|Filtrar con *.zip|Descargar selección|Capturar descargas del navegador|El navegador sigue descargando hasta que Flux confirma la recepción.|Integración del navegador|Abre los ajustes de Flux e instala el enlace con el navegador.|Guardado|Escritorio desconectado|Esperando al escritorio|No se detectó contenido|Reproduce un vídeo para detectar sus solicitudes.|Descarga por lotes|Prioridad|Iniciar a las|Detener a las|Procesar contenido con FFmpeg|URL de audio separado (opcional)|Duración de grabación (segundos, opcional)|Contenedor|Descargas simultáneas|Carpeta de descargas|Elegir carpeta|Ejecutar cola|Detectar enlaces del portapapeles|Minimizar a la bandeja|Avisos de finalización|Guardar ajustes|Abrir archivo|Mostrar en carpeta|Eliminar tarea|¿Eliminar la tarea y sus datos temporales? Se conserva el archivo terminado.|Eliminar|Analizar pistas|Iniciar cola|Pausar todo|Instalar enlace del navegador|Enlace del navegador instalado|Copiar SHA-256",
  "ar-SA":
    "تنزيل الوسائط|تنزيل باستخدام Flux|التقاط روابط الصفحة|الجودة / الترميز|تلقائي|جارٍ فحص الوسائط…|تم الحفظ في قائمة سطح المكتب|إعادة المحاولة|تصفية باستخدام *.zip|تنزيل المحدد|التقاط تنزيلات المتصفح|يواصل المتصفح التنزيل حتى يؤكد Flux الاستلام.|تكامل المتصفح|افتح إعدادات Flux على سطح المكتب وثبّت جسر المتصفح.|تم الحفظ|سطح المكتب غير متصل|بانتظار سطح المكتب|لم تُكتشف وسائط|شغّل فيديو لاكتشاف طلبات الوسائط.|تنزيل جماعي|الأولوية|وقت البدء|وقت الإيقاف|معالجة الوسائط باستخدام FFmpeg|رابط صوت منفصل (اختياري)|مدة التسجيل (بالثواني، اختياري)|الحاوية|التنزيلات المتزامنة|مجلد التنزيلات|اختيار مجلد|تشغيل القائمة|مراقبة روابط الحافظة|تصغير إلى شريط النظام|إشعارات الاكتمال|حفظ الإعدادات|فتح الملف|عرض في المجلد|إزالة المهمة|إزالة المهمة وبياناتها المؤقتة؟ يُحتفظ بالملف المكتمل.|إزالة|فحص المسارات|بدء القائمة|إيقاف الكل مؤقتاً|تثبيت جسر المتصفح|تم تثبيت جسر المتصفح|نسخ SHA-256",
  "fr-FR":
    "Télécharger le média|Télécharger avec Flux|Capturer les liens de la page|Qualité / codec|Automatique|Analyse du média…|Enregistré dans la file de bureau|Réessayer|Filtrer avec *.zip|Télécharger la sélection|Intercepter les téléchargements|Le navigateur continue jusqu’à la confirmation de Flux.|Intégration du navigateur|Ouvrez les paramètres de Flux et installez le relais du navigateur.|Enregistré|Application hors ligne|En attente du bureau|Aucun média détecté|Lancez une vidéo pour détecter ses requêtes.|Téléchargement par lot|Priorité|Démarrer à|Arrêter à|Traiter le média avec FFmpeg|URL audio distincte (facultatif)|Durée d’enregistrement (secondes, facultatif)|Conteneur|Téléchargements simultanés|Dossier de téléchargement|Choisir un dossier|Exécuter la file|Surveiller les liens du presse-papiers|Réduire dans la zone de notification|Notifications de fin|Enregistrer les paramètres|Ouvrir le fichier|Afficher dans le dossier|Supprimer la tâche|Supprimer la tâche et ses données temporaires ? Le fichier terminé est conservé.|Supprimer|Analyser les pistes|Démarrer la file|Tout suspendre|Installer le relais du navigateur|Relais du navigateur installé|Copier SHA-256",
  "bn-BD":
    "মিডিয়া ডাউনলোড|Flux দিয়ে ডাউনলোড|পৃষ্ঠার লিংক সংগ্রহ|মান / কোডেক|স্বয়ংক্রিয়|মিডিয়া পরীক্ষা হচ্ছে…|ডেস্কটপ সারিতে সংরক্ষিত|আবার চেষ্টা|*.zip দিয়ে ছাঁকুন|নির্বাচিতগুলি ডাউনলোড|ব্রাউজারের ডাউনলোড ধরুন|Flux নিশ্চিত না করা পর্যন্ত ব্রাউজার ডাউনলোড চালায়।|ব্রাউজার সংযোগ|Flux ডেস্কটপ সেটিংস খুলে ব্রাউজার ব্রিজ স্থাপন করুন।|সংরক্ষিত|ডেস্কটপ অফলাইন|ডেস্কটপের অপেক্ষায়|মিডিয়া পাওয়া যায়নি|মিডিয়া অনুরোধ শনাক্ত করতে ভিডিও চালান।|একসঙ্গে একাধিক ডাউনলোড|অগ্রাধিকার|শুরুর সময়|থামার সময়|FFmpeg দিয়ে মিডিয়া প্রক্রিয়াকরণ|আলাদা অডিও URL (ঐচ্ছিক)|রেকর্ডিংয়ের সময় (সেকেন্ড, ঐচ্ছিক)|কন্টেইনার|সমসাময়িক ডাউনলোড|ডাউনলোড ফোল্ডার|ফোল্ডার বাছুন|সারি চালান|ক্লিপবোর্ডের লিংক দেখুন|ট্রেতে ছোট করুন|সমাপ্তির বিজ্ঞপ্তি|সেটিংস সংরক্ষণ|ফাইল খুলুন|ফোল্ডারে দেখান|কাজ সরান|কাজ ও অস্থায়ী তথ্য সরাবেন? সম্পূর্ণ ফাইল রাখা হবে।|সরান|স্ট্রিম পরীক্ষা|সারি শুরু|সব থামান|ব্রাউজার ব্রিজ স্থাপন|ব্রাউজার ব্রিজ স্থাপিত|SHA-256 কপি",
  "pt-BR":
    "Baixar mídia|Baixar com Flux|Capturar links da página|Qualidade / codec|Automático|Analisando mídia…|Salvo na fila do aplicativo|Tentar novamente|Filtrar com *.zip|Baixar selecionados|Interceptar downloads do navegador|O navegador continua baixando até a confirmação do Flux.|Integração com navegador|Abra as configurações do Flux e instale a ponte do navegador.|Salvo|Aplicativo desconectado|Aguardando aplicativo|Nenhuma mídia detectada|Reproduza um vídeo para detectar suas solicitações.|Download em lote|Prioridade|Iniciar em|Parar em|Processar mídia com FFmpeg|URL de áudio separado (opcional)|Duração da gravação (segundos, opcional)|Contêiner|Downloads simultâneos|Pasta de downloads|Escolher pasta|Executar fila|Monitorar links da área de transferência|Minimizar para a bandeja|Notificações de conclusão|Salvar configurações|Abrir arquivo|Mostrar na pasta|Remover tarefa|Remover a tarefa e seus dados temporários? O arquivo concluído será mantido.|Remover|Analisar faixas|Iniciar fila|Pausar tudo|Instalar ponte do navegador|Ponte do navegador instalada|Copiar SHA-256",
  "ru-RU":
    "Скачать медиа|Скачать через Flux|Собрать ссылки страницы|Качество / кодек|Автоматически|Анализ медиа…|Сохранено в очереди приложения|Повторить|Фильтр *.zip|Скачать выбранное|Перехватывать загрузки браузера|Браузер продолжает загрузку до подтверждения Flux.|Интеграция с браузером|Откройте настройки Flux и установите мост браузера.|Сохранено|Приложение не подключено|Ожидание приложения|Медиа не найдено|Запустите видео для обнаружения запросов.|Пакетная загрузка|Приоритет|Время запуска|Время остановки|Обработать медиа через FFmpeg|Отдельный URL аудио (необязательно)|Длительность записи (секунды, необязательно)|Контейнер|Одновременные загрузки|Папка загрузок|Выбрать папку|Запустить очередь|Отслеживать ссылки буфера обмена|Свернуть в трей|Уведомления о завершении|Сохранить настройки|Открыть файл|Показать в папке|Удалить задачу|Удалить задачу и временные данные? Готовый файл сохранится.|Удалить|Анализ потоков|Запустить очередь|Приостановить всё|Установить мост браузера|Мост браузера установлен|Копировать SHA-256",
  "ur-PK":
    "میڈیا ڈاؤن لوڈ|Flux کے ذریعے ڈاؤن لوڈ|صفحے کے روابط جمع کریں|معیار / کوڈیک|خودکار|میڈیا کی جانچ جاری…|ڈیسک ٹاپ قطار میں محفوظ|دوبارہ کوشش|*.zip سے چھانٹیں|منتخب ڈاؤن لوڈ کریں|براؤزر ڈاؤن لوڈ سنبھالیں|Flux کی تصدیق تک براؤزر ڈاؤن لوڈ جاری رکھتا ہے۔|براؤزر انضمام|Flux ڈیسک ٹاپ ترتیبات کھول کر براؤزر برج نصب کریں۔|محفوظ ہو گیا|ڈیسک ٹاپ آف لائن|ڈیسک ٹاپ کا انتظار|میڈیا نہیں ملا|میڈیا درخواستیں شناخت کرنے کے لیے ویڈیو چلائیں۔|اجتماعی ڈاؤن لوڈ|ترجیح|آغاز کا وقت|روکنے کا وقت|FFmpeg سے میڈیا پراسیس کریں|الگ آڈیو URL (اختیاری)|ریکارڈنگ دورانیہ (سیکنڈ، اختیاری)|کنٹینر|بیک وقت ڈاؤن لوڈ|ڈاؤن لوڈ فولڈر|فولڈر منتخب کریں|قطار چلائیں|کلپ بورڈ روابط دیکھیں|ٹرے میں چھوٹا کریں|تکمیل کی اطلاعات|ترتیبات محفوظ کریں|فائل کھولیں|فولڈر میں دکھائیں|کام ہٹائیں|کام اور عارضی ڈیٹا ہٹائیں؟ مکمل فائل برقرار رہے گی۔|ہٹائیں|اسٹریم جانچیں|قطار شروع کریں|سب روکیں|براؤزر برج نصب کریں|براؤزر برج نصب ہو گیا|SHA-256 نقل کریں",
  "id-ID":
    "Unduh media|Unduh dengan Flux|Ambil tautan halaman|Kualitas / codec|Otomatis|Memeriksa media…|Disimpan ke antrean desktop|Coba lagi|Filter dengan *.zip|Unduh pilihan|Tangkap unduhan browser|Browser tetap mengunduh sampai Flux mengonfirmasi penerimaan.|Integrasi browser|Buka pengaturan Flux dan pasang penghubung browser.|Tersimpan|Desktop tidak terhubung|Menunggu desktop|Media tidak ditemukan|Putar video untuk mendeteksi permintaan media.|Unduhan massal|Prioritas|Mulai pada|Berhenti pada|Proses media dengan FFmpeg|URL audio terpisah (opsional)|Durasi rekaman (detik, opsional)|Kontainer|Unduhan bersamaan|Folder unduhan|Pilih folder|Jalankan antrean|Pantau tautan papan klip|Minimalkan ke baki|Notifikasi selesai|Simpan pengaturan|Buka berkas|Tampilkan di folder|Hapus tugas|Hapus tugas dan data sementara? Berkas selesai tetap disimpan.|Hapus|Periksa stream|Mulai antrean|Jeda semua|Pasang penghubung browser|Penghubung browser terpasang|Salin SHA-256",
  "de-DE":
    "Medien herunterladen|Mit Flux herunterladen|Seitenlinks erfassen|Qualität / Codec|Automatisch|Medien werden geprüft…|In der Desktop-Warteschlange gespeichert|Erneut versuchen|Mit *.zip filtern|Auswahl herunterladen|Browser-Downloads übernehmen|Der Browser lädt weiter, bis Flux den Empfang bestätigt.|Browserintegration|Öffne die Flux-Einstellungen und installiere die Browserbrücke.|Gespeichert|Desktop nicht verbunden|Warte auf Desktop|Keine Medien erkannt|Starte ein Video, um Medienanfragen zu erkennen.|Stapel-Download|Priorität|Startzeit|Stoppzeit|Medien mit FFmpeg verarbeiten|Separate Audio-URL (optional)|Aufnahmedauer (Sekunden, optional)|Container|Gleichzeitige Downloads|Downloadordner|Ordner auswählen|Warteschlange ausführen|Zwischenablage auf Links prüfen|In Infobereich minimieren|Benachrichtigung bei Abschluss|Einstellungen speichern|Datei öffnen|Im Ordner anzeigen|Aufgabe entfernen|Aufgabe und temporäre Daten entfernen? Die fertige Datei bleibt erhalten.|Entfernen|Streams prüfen|Warteschlange starten|Alle pausieren|Browserbrücke installieren|Browserbrücke installiert|SHA-256 kopieren",
  "ja-JP":
    "メディアをダウンロード|Fluxでダウンロード|ページのリンクを取得|画質 / コーデック|自動|メディアを解析中…|デスクトップのキューに保存しました|再試行|*.zipで絞り込み|選択した項目をダウンロード|ブラウザのダウンロードを引き継ぐ|Fluxの受信確認までブラウザはダウンロードを続けます。|ブラウザ連携|Fluxの設定を開き、ブラウザブリッジをインストールしてください。|保存しました|デスクトップ未接続|デスクトップを待機中|メディアが見つかりません|動画を再生してメディア通信を検出してください。|一括ダウンロード|優先度|開始時刻|停止時刻|FFmpegでメディアを処理|別の音声URL（任意）|録画時間（秒、任意）|コンテナ|同時ダウンロード数|ダウンロードフォルダ|フォルダを選択|キューを実行|クリップボードのリンクを検出|トレイに最小化|完了通知|設定を保存|ファイルを開く|フォルダに表示|タスクを削除|タスクと一時データを削除しますか？完成したファイルは残ります。|削除|ストリームを解析|キューを開始|すべて一時停止|ブラウザブリッジをインストール|ブラウザブリッジをインストールしました|SHA-256をコピー",
  "pcm-NG":
    "Download media|Download with Flux|Collect page links|Quality / codec|Automatic|Dey check media…|E don save for desktop queue|Try again|Filter with *.zip|Download wetin you choose|Catch browser downloads|Browser go continue download until Flux confirm say e receive am.|Browser connection|Open Flux desktop settings and install browser bridge.|E don save|Desktop no connect|Dey wait for desktop|No media dey|Play video make we see the media requests.|Download plenty files|Priority|Start time|Stop time|Use FFmpeg process media|Separate audio URL (if you get)|Recording time (seconds, optional)|Container|Downloads wey fit run together|Download folder|Choose folder|Run queue|Check clipboard links|Minimize go tray|Tell me when e finish|Save settings|Open file|Show for folder|Remove task|Remove this task and temporary data? The file wey don finish go remain.|Remove|Check streams|Start queue|Pause everything|Install browser bridge|Browser bridge don install|Copy SHA-256",
  "mr-IN":
    "माध्यम डाउनलोड|Flux सह डाउनलोड|पानावरील दुवे गोळा करा|गुणवत्ता / कोडेक|स्वयंचलित|माध्यम तपासत आहे…|डेस्कटॉप रांगेत जतन केले|पुन्हा प्रयत्न|*.zip ने गाळा|निवडलेले डाउनलोड|ब्राउझर डाउनलोड हाताळा|Flux पुष्टी करेपर्यंत ब्राउझर डाउनलोड चालू ठेवतो.|ब्राउझर एकत्रीकरण|Flux डेस्कटॉप सेटिंग उघडा आणि ब्राउझर ब्रिज स्थापित करा.|जतन केले|डेस्कटॉप ऑफलाइन|डेस्कटॉपची प्रतीक्षा|माध्यम आढळले नाही|माध्यम विनंत्या शोधण्यासाठी व्हिडिओ चालवा.|समूह डाउनलोड|प्राधान्य|सुरू होण्याची वेळ|थांबण्याची वेळ|FFmpeg ने माध्यम प्रक्रिया|वेगळा ऑडिओ URL (पर्यायी)|रेकॉर्डिंग वेळ (सेकंद, पर्यायी)|कंटेनर|एकाचवेळी डाउनलोड|डाउनलोड फोल्डर|फोल्डर निवडा|रांग चालवा|क्लिपबोर्ड दुवे तपासा|ट्रेमध्ये लहान करा|पूर्ण झाल्याच्या सूचना|सेटिंग जतन करा|फाइल उघडा|फोल्डरमध्ये दाखवा|कार्य काढा|कार्य आणि तात्पुरता डेटा काढायचा? पूर्ण फाइल ठेवली जाईल.|काढा|स्ट्रीम तपासा|रांग सुरू करा|सर्व थांबवा|ब्राउझर ब्रिज स्थापित करा|ब्राउझर ब्रिज स्थापित|SHA-256 कॉपी करा",
  "te-IN":
    "మీడియా డౌన్‌లోడ్|Flux తో డౌన్‌లోడ్|పేజీ లింకులను సేకరించండి|నాణ్యత / కోడెక్|స్వయంచాలకంగా|మీడియాను పరిశీలిస్తోంది…|డెస్క్‌టాప్ వరుసలో సేవ్ అయింది|మళ్లీ ప్రయత్నించండి|*.zip తో వడపోయండి|ఎంచుకున్నవి డౌన్‌లోడ్|బ్రౌజర్ డౌన్‌లోడ్‌లను స్వీకరించండి|Flux నిర్ధారించే వరకు బ్రౌజర్ డౌన్‌లోడ్ కొనసాగుతుంది.|బ్రౌజర్ అనుసంధానం|Flux డెస్క్‌టాప్ సెట్టింగ్‌లు తెరిచి బ్రౌజర్ బ్రిడ్జ్ ఇన్‌స్టాల్ చేయండి.|సేవ్ అయింది|డెస్క్‌టాప్ ఆఫ్‌లైన్|డెస్క్‌టాప్ కోసం వేచి ఉంది|మీడియా కనిపించలేదు|మీడియా అభ్యర్థనలు గుర్తించడానికి వీడియో ప్లే చేయండి.|సామూహిక డౌన్‌లోడ్|ప్రాధాన్యత|ప్రారంభ సమయం|ఆపే సమయం|FFmpeg తో మీడియా ప్రాసెస్|వేరే ఆడియో URL (ఐచ్ఛికం)|రికార్డింగ్ వ్యవధి (సెకన్లు, ఐచ్ఛికం)|కంటైనర్|ఏకకాల డౌన్‌లోడ్‌లు|డౌన్‌లోడ్ ఫోల్డర్|ఫోల్డర్ ఎంచుకోండి|వరుసను అమలు చేయండి|క్లిప్‌బోర్డ్ లింకులు గమనించండి|ట్రేకు కనిష్టీకరించండి|పూర్తయిన నోటిఫికేషన్‌లు|సెట్టింగ్‌లు సేవ్ చేయండి|ఫైల్ తెరవండి|ఫోల్డర్‌లో చూపండి|పనిని తొలగించండి|పని మరియు తాత్కాలిక డేటా తొలగించాలా? పూర్తయిన ఫైల్ ఉంటుంది.|తొలగించండి|స్ట్రీమ్‌లను పరిశీలించండి|వరుస ప్రారంభించండి|అన్నీ పాజ్ చేయండి|బ్రౌజర్ బ్రిడ్జ్ ఇన్‌స్టాల్|బ్రౌజర్ బ్రిడ్జ్ ఇన్‌స్టాల్ అయింది|SHA-256 కాపీ",
  "tr-TR":
    "Medyayı indir|Flux ile indir|Sayfa bağlantılarını yakala|Kalite / kodek|Otomatik|Medya inceleniyor…|Masaüstü kuyruğuna kaydedildi|Yeniden dene|*.zip ile filtrele|Seçilenleri indir|Tarayıcı indirmelerini devral|Flux alındığını onaylayana kadar tarayıcı indirmeye devam eder.|Tarayıcı bütünleşmesi|Flux masaüstü ayarlarını açıp tarayıcı köprüsünü kurun.|Kaydedildi|Masaüstü çevrimdışı|Masaüstü bekleniyor|Medya bulunamadı|Medya isteklerini algılamak için videoyu oynatın.|Toplu indirme|Öncelik|Başlangıç zamanı|Durdurma zamanı|Medyayı FFmpeg ile işle|Ayrı ses URL’si (isteğe bağlı)|Kayıt süresi (saniye, isteğe bağlı)|Kapsayıcı|Eşzamanlı indirmeler|İndirme klasörü|Klasör seç|Kuyruğu çalıştır|Panodaki bağlantıları izle|Sistem tepsisine küçült|Tamamlanma bildirimleri|Ayarları kaydet|Dosyayı aç|Klasörde göster|Görevi kaldır|Görev ve geçici verileri kaldırılsın mı? Tamamlanan dosya korunur.|Kaldır|Akışları incele|Kuyruğu başlat|Tümünü duraklat|Tarayıcı köprüsünü kur|Tarayıcı köprüsü kuruldu|SHA-256’yı kopyala",
  "ta-IN":
    "ஊடகத்தைப் பதிவிறக்கு|Flux மூலம் பதிவிறக்கு|பக்க இணைப்புகளைச் சேகரி|தரம் / கோடெக்|தானியங்கி|ஊடகத்தை ஆய்வு செய்கிறது…|டெஸ்க்டாப் வரிசையில் சேமிக்கப்பட்டது|மீண்டும் முயற்சி|*.zip மூலம் வடிகட்டு|தேர்ந்தவற்றைப் பதிவிறக்கு|உலாவி பதிவிறக்கங்களை ஏற்றுக்கொள்|Flux உறுதிசெய்யும் வரை உலாவி பதிவிறக்கத்தைத் தொடரும்.|உலாவி ஒருங்கிணைப்பு|Flux டெஸ்க்டாப் அமைப்புகளைத் திறந்து உலாவி பாலத்தை நிறுவவும்.|சேமிக்கப்பட்டது|டெஸ்க்டாப் இணையமற்றது|டெஸ்க்டாப்புக்குக் காத்திருக்கிறது|ஊடகம் கண்டறியப்படவில்லை|ஊடகக் கோரிக்கைகளைக் கண்டறிய காணொளியை இயக்கவும்.|தொகுப்புப் பதிவிறக்கம்|முன்னுரிமை|தொடக்க நேரம்|நிறுத்தும் நேரம்|FFmpeg மூலம் ஊடகத்தைச் செயலாக்கு|தனி ஒலி URL (விருப்பம்)|பதிவு நேரம் (விநாடிகள், விருப்பம்)|கொள்கலன்|ஒரே நேரப் பதிவிறக்கங்கள்|பதிவிறக்கக் கோப்புறை|கோப்புறையைத் தேர்வு செய்|வரிசையை இயக்கு|கிளிப்போர்டு இணைப்புகளைக் கவனி|தட்டில் சிறிதாக்கு|முடிவு அறிவிப்புகள்|அமைப்புகளைச் சேமி|கோப்பைத் திற|கோப்புறையில் காட்டு|பணியை அகற்று|பணியையும் தற்காலிகத் தரவையும் அகற்றவா? முடிந்த கோப்பு இருக்கும்.|அகற்று|ஸ்ட்ரீம்களை ஆய்வு செய்|வரிசையைத் தொடங்கு|அனைத்தையும் இடைநிறுத்து|உலாவி பாலத்தை நிறுவு|உலாவி பாலம் நிறுவப்பட்டது|SHA-256 நகலெடு",
  "yue-HK":
    "下載媒體|用Flux下載|擷取網頁連結|畫質 / 編碼|自動|檢查媒體中…|已儲存至桌面佇列|再試|用*.zip篩選|下載所選項目|接管瀏覽器下載|Flux確認收到之前，瀏覽器會繼續下載。|瀏覽器整合|開啟Flux桌面設定，再安裝瀏覽器橋接。|已儲存|桌面程式離線|等候桌面程式|搵唔到媒體|播放影片以偵測媒體請求。|批次下載|優先次序|開始時間|停止時間|用FFmpeg處理媒體|獨立音訊網址（可選）|錄影時間（秒，可選）|封裝格式|同時下載數目|下載資料夾|揀資料夾|執行佇列|監察剪貼簿連結|縮小至系統匣|完成通知|儲存設定|開啟檔案|喺資料夾顯示|移除工作|移除工作同暫存資料？已完成檔案會保留。|移除|檢查串流|啟動佇列|全部暫停|安裝瀏覽器橋接|瀏覽器橋接已安裝|複製SHA-256",
  "vi-VN":
    "Tải nội dung đa phương tiện|Tải bằng Flux|Thu thập liên kết trang|Chất lượng / codec|Tự động|Đang kiểm tra nội dung…|Đã lưu vào hàng đợi máy tính|Thử lại|Lọc bằng *.zip|Tải mục đã chọn|Tiếp nhận tải xuống trình duyệt|Trình duyệt tiếp tục tải đến khi Flux xác nhận đã nhận.|Tích hợp trình duyệt|Mở cài đặt Flux và cài cầu nối trình duyệt.|Đã lưu|Ứng dụng ngoại tuyến|Đang chờ ứng dụng|Không tìm thấy nội dung|Phát video để phát hiện yêu cầu nội dung.|Tải hàng loạt|Ưu tiên|Thời gian bắt đầu|Thời gian dừng|Xử lý nội dung bằng FFmpeg|URL âm thanh riêng (tùy chọn)|Thời lượng ghi (giây, tùy chọn)|Định dạng chứa|Tải xuống đồng thời|Thư mục tải xuống|Chọn thư mục|Chạy hàng đợi|Theo dõi liên kết bộ nhớ tạm|Thu nhỏ vào khay|Thông báo hoàn tất|Lưu cài đặt|Mở tệp|Hiện trong thư mục|Xóa tác vụ|Xóa tác vụ và dữ liệu tạm? Tệp hoàn tất được giữ lại.|Xóa|Kiểm tra luồng|Bắt đầu hàng đợi|Tạm dừng tất cả|Cài cầu nối trình duyệt|Đã cài cầu nối trình duyệt|Sao chép SHA-256",
};
const extras = {
  "en-US": ["Apply", "TS segment URLs (one per line, optional)"],
  "zh-CN": ["应用", "TS分段地址（每行一个，可选）"],
  "hi-IN": ["लागू करें", "TS खंड URL (प्रति पंक्ति एक, वैकल्पिक)"],
  "es-ES": ["Aplicar", "URL de segmentos TS (una por línea, opcional)"],
  "ar-SA": ["تطبيق", "روابط مقاطع TS (رابط لكل سطر، اختياري)"],
  "fr-FR": ["Appliquer", "URL des segments TS (une par ligne, facultatif)"],
  "bn-BD": ["প্রয়োগ করুন", "TS খণ্ডের URL (প্রতি লাইনে একটি, ঐচ্ছিক)"],
  "pt-BR": ["Aplicar", "URLs dos segmentos TS (uma por linha, opcional)"],
  "ru-RU": [
    "Применить",
    "URL сегментов TS (по одному на строку, необязательно)",
  ],
  "ur-PK": ["لاگو کریں", "TS حصوں کے URL (فی سطر ایک، اختیاری)"],
  "id-ID": ["Terapkan", "URL segmen TS (satu per baris, opsional)"],
  "de-DE": ["Anwenden", "TS-Segment-URLs (eine pro Zeile, optional)"],
  "ja-JP": ["適用", "TSセグメントURL（1行に1件、任意）"],
  "pcm-NG": ["Apply", "TS segment URL (one for each line, if you want)"],
  "mr-IN": ["लागू करा", "TS भागांचे URL (प्रत्येक ओळीत एक, ऐच्छिक)"],
  "te-IN": ["వర్తింపజేయి", "TS భాగాల URLలు (పంక్తికి ఒకటి, ఐచ్ఛికం)"],
  "tr-TR": ["Uygula", "TS parça adresleri (satır başına bir, isteğe bağlı)"],
  "ta-IN": [
    "பயன்படுத்து",
    "TS பகுதிகளின் URL (வரிக்கு ஒன்று, விருப்பத்தேர்வு)",
  ],
  "yue-HK": ["套用", "TS分段網址（每行一個，可選）"],
  "vi-VN": ["Áp dụng", "URL đoạn TS (mỗi dòng một URL, tùy chọn)"],
};
const chromeLocales = {
  "en-US": "en",
  "zh-CN": "zh_CN",
  "hi-IN": "hi",
  "es-ES": "es",
  "ar-SA": "ar",
  "fr-FR": "fr",
  "bn-BD": "bn",
  "pt-BR": "pt_BR",
  "ru-RU": "ru",
  "ur-PK": "ur",
  "id-ID": "id",
  "de-DE": "de",
  "ja-JP": "ja",
  "mr-IN": "mr",
  "te-IN": "te",
  "tr-TR": "tr",
  "ta-IN": "ta",
  "vi-VN": "vi",
  "yue-HK": "zh_TW",
};
const globalLimits = {
  "en-US": "Total speed limit (bytes/s, 0 = unlimited)",
  "zh-CN": "总速度限制（字节/秒，0 = 不限）",
  "hi-IN": "कुल गति सीमा (बाइट/सेकंड, 0 = असीमित)",
  "es-ES": "Límite de velocidad total (bytes/s, 0 = sin límite)",
  "ar-SA": "حد السرعة الإجمالي (بايت/ثانية، 0 = بلا حد)",
  "fr-FR": "Limite de débit total (octets/s, 0 = illimité)",
  "bn-BD": "মোট গতির সীমা (বাইট/সেকেন্ড, 0 = সীমাহীন)",
  "pt-BR": "Limite de velocidade total (bytes/s, 0 = ilimitado)",
  "ru-RU": "Общий лимит скорости (байт/с, 0 = без ограничений)",
  "ur-PK": "مجموعی رفتار کی حد (بائٹ/سیکنڈ، 0 = لامحدود)",
  "id-ID": "Batas kecepatan total (byte/dtk, 0 = tanpa batas)",
  "de-DE": "Gesamtes Geschwindigkeitslimit (Byte/s, 0 = unbegrenzt)",
  "ja-JP": "合計速度制限（バイト/秒、0 = 無制限）",
  "pcm-NG": "Total speed limit (bytes/s, 0 = no limit)",
  "mr-IN": "एकूण वेग मर्यादा (बाइट/सेकंद, 0 = अमर्यादित)",
  "te-IN": "మొత్తం వేగ పరిమితి (బైట్లు/సెకను, 0 = అపరిమితం)",
  "tr-TR": "Toplam hız sınırı (bayt/sn, 0 = sınırsız)",
  "ta-IN": "மொத்த வேக வரம்பு (பைட்/வி, 0 = வரம்பில்லை)",
  "yue-HK": "總速度限制（位元組/秒，0 = 無限制）",
  "vi-VN": "Giới hạn tốc độ tổng (byte/giây, 0 = không giới hạn)",
};
for (const [code, text] of Object.entries(catalogs)) {
  const values = text.split("|");
  if (values.length !== keys.length)
    throw new Error(`${code}: ${values.length}/${keys.length}`);
  const path = `src/locales/${code}.json`;
  const dictionary = {
    ...JSON.parse(readFileSync(path)),
    ...Object.fromEntries(keys.map((key, index) => [key, values[index]])),
    apply: extras[code][0],
    segment_urls: extras[code][1],
    global_speed_limit: globalLimits[code],
  };
  writeFileSync(path, JSON.stringify(dictionary, null, 2) + "\n");
  mkdirSync("extension/locales", { recursive: true });
  writeFileSync(
    `extension/locales/${code}.json`,
    JSON.stringify(dictionary, null, 2) + "\n",
  );
  if (chromeLocales[code]) {
    const directory = `extension/_locales/${chromeLocales[code]}`;
    mkdirSync(directory, { recursive: true });
    const messages = Object.fromEntries(
      Object.entries(dictionary).map(([key, message]) => [key, { message }]),
    );
    messages.appName = {
      message: storeMetadata.names?.[chromeLocales[code]] || storeMetadata.name,
    };
    messages.appDescription = {
      message:
        storeMetadata.descriptions[chromeLocales[code]] ||
        dictionary.add_description,
    };
    writeFileSync(
      `${directory}/messages.json`,
      JSON.stringify(messages, null, 2) + "\n",
    );
  }
}
const source = readFileSync("src/i18n.ts", "utf8");
const languages = [
  ...source.matchAll(
    /\{ code: "([^"]+)", name: "([^"]+)", native: "([^"]+)" \}/g,
  ),
].map(([, code, name, native]) => ({ code, name, native }));
if (languages.length !== 20) throw new Error("Expected 20 languages");
writeFileSync(
  "extension/languages.json",
  JSON.stringify(languages, null, 2) + "\n",
);
mkdirSync("extension/icons", { recursive: true });
copyFileSync("src-tauri/icons/icon.png", "extension/icons/128.png");
const manifest = JSON.parse(readFileSync("extension/manifest.json"));
if (!manifest.key)
  manifest.key = generateKeyPairSync("rsa", { modulusLength: 2048 })
    .publicKey.export({ type: "spki", format: "der" })
    .toString("base64");
const id = [
  ...createHash("sha256")
    .update(Buffer.from(manifest.key, "base64"))
    .digest()
    .subarray(0, 16),
]
  .map((byte) => String.fromCharCode(97 + (byte >> 4), 97 + (byte & 15)))
  .join("");
writeFileSync(
  "extension/manifest.json",
  JSON.stringify(manifest, null, 2) + "\n",
);
writeFileSync("extension/extension-id.txt", id + "\n");
console.log(`20 catalogs extended; Chrome extension ID: ${id}`);
