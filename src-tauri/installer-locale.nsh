!macro FLUX_SAVE_INSTALLER_LOCALE
  ${Unless} ${Silent}
  ${AndIf} $UpdateMode != 1
  ${AndIf} $PassiveMode != 1
    StrCpy $1 "en-US"
    ${If} $LANGUAGE == 1033
      StrCpy $1 "en-US"
    ${EndIf}
    ${If} $LANGUAGE == 2052
      StrCpy $1 "zh-CN"
    ${EndIf}
    ${If} $LANGUAGE == 1081
      StrCpy $1 "hi-IN"
    ${EndIf}
    ${If} $LANGUAGE == 1034
      StrCpy $1 "es-ES"
    ${EndIf}
    ${If} $LANGUAGE == 1025
      StrCpy $1 "ar-SA"
    ${EndIf}
    ${If} $LANGUAGE == 1036
      StrCpy $1 "fr-FR"
    ${EndIf}
    ${If} $LANGUAGE == 1093
      StrCpy $1 "bn-BD"
    ${EndIf}
    ${If} $LANGUAGE == 1046
      StrCpy $1 "pt-BR"
    ${EndIf}
    ${If} $LANGUAGE == 1049
      StrCpy $1 "ru-RU"
    ${EndIf}
    ${If} $LANGUAGE == 1056
      StrCpy $1 "ur-PK"
    ${EndIf}
    ${If} $LANGUAGE == 1057
      StrCpy $1 "id-ID"
    ${EndIf}
    ${If} $LANGUAGE == 1031
      StrCpy $1 "de-DE"
    ${EndIf}
    ${If} $LANGUAGE == 1041
      StrCpy $1 "ja-JP"
    ${EndIf}
    ${If} $LANGUAGE == 4096
      StrCpy $1 "pcm-NG"
    ${EndIf}
    ${If} $LANGUAGE == 1102
      StrCpy $1 "mr-IN"
    ${EndIf}
    ${If} $LANGUAGE == 1098
      StrCpy $1 "te-IN"
    ${EndIf}
    ${If} $LANGUAGE == 1055
      StrCpy $1 "tr-TR"
    ${EndIf}
    ${If} $LANGUAGE == 1097
      StrCpy $1 "ta-IN"
    ${EndIf}
    ${If} $LANGUAGE == 1028
      StrCpy $1 "yue-HK"
    ${EndIf}
    ${If} $LANGUAGE == 1066
      StrCpy $1 "vi-VN"
    ${EndIf}
    FileOpen $0 "$INSTDIR\installer-locale.txt" w
    FileWrite $0 "$1"
    FileClose $0
  ${EndUnless}
!macroend
