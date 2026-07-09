/**
 * The bundled theme-pack descriptors — the client-side stand-in for the deferred
 * server themes service (GET /api/v1/themes). Generated from each pack's
 * public/themes/<id>/{theme,tokens}.json at port time (regenerate by re-running
 * the generator in the P4.5 commit). The ThemeService reads these as its
 * listThemes descriptors and applies a pack by id; when the server themes
 * service lands (Settings vertical), this array is replaced by that server read.
 */

export interface ThemeFontFace {
  family: string;
  src: string;
  weight: string;
  style: string;
  display: string;
}

export interface ThemePreviewColors {
  background: string;
  primary: string;
  secondary: string;
}

export interface ThemeDescriptor {
  id: string;
  name: string;
  description: string;
  supportsDarkMode: boolean;
  previewColors: ThemePreviewColors;
  fonts: ThemeFontFace[];
}

/** The six bundled packs, in v4 registration order. */
export const BUNDLED_THEMES: ThemeDescriptor[] = [
  {
    "id": "art-deco",
    "name": "Art Deco",
    "description": "The geometry of elegance, the precision of grandeur. Navy-and-gold palette evoking the Chrysler Building lobby at dusk — opulent but disciplined. Geometric sans-serif headings with wide tracking, elegant serif body text, and gold accents throughout. Light mode is ivory and gold leaf; dark mode is midnight and burnished brass.",
    "supportsDarkMode": true,
    "previewColors": {
      "background": "hsl(45 28% 78%)",
      "primary": "hsl(222 47% 18%)",
      "secondary": "hsl(222 20% 68%)"
    },
    "fonts": [
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Light.woff2",
        "weight": "300",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Medium.woff2",
        "weight": "500",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Cormorant Garamond",
        "src": "fonts/CormorantGaramond-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Cormorant Garamond",
        "src": "fonts/CormorantGaramond-Italic.woff2",
        "weight": "400",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Cormorant Garamond",
        "src": "fonts/CormorantGaramond-Medium.woff2",
        "weight": "500",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Cormorant Garamond",
        "src": "fonts/CormorantGaramond-MediumItalic.woff2",
        "weight": "500",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Cormorant Garamond",
        "src": "fonts/CormorantGaramond-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      }
    ]
  },
  {
    "id": "earl-grey",
    "name": "Earl Grey",
    "description": "A high-contrast dark theme inspired by modern AI chat interfaces, with pure black sidebar, charcoal main area, and clean minimal styling",
    "supportsDarkMode": true,
    "previewColors": {
      "background": "hsl(0 0% 100%)",
      "primary": "hsl(0 0% 5%)",
      "secondary": "hsl(0 0% 96%)"
    },
    "fonts": [
      {
        "family": "Rethink Sans",
        "src": "fonts/RethinkSans-latin.woff2",
        "weight": "400 700",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Rethink Sans",
        "src": "fonts/RethinkSans-latin-ext.woff2",
        "weight": "400 700",
        "style": "normal",
        "display": "swap"
      }
    ]
  },
  {
    "id": "great-estate",
    "name": "The Great Estate",
    "description": "Warm gold and mahogany, like a manor house library at golden hour. Playfair Display serif headings with Inter sans-serif body text, carbon-fibre texture overlay, and a palette that whispers old money. Light mode is cream and vellum; dark mode is deep walnut and burnished gold.",
    "supportsDarkMode": true,
    "previewColors": {
      "background": "hsl(38 30% 97%)",
      "primary": "hsl(20 45% 25%)",
      "secondary": "hsl(38 18% 88%)"
    },
    "fonts": [
      {
        "family": "Playfair Display",
        "src": "fonts/PlayfairDisplay-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Playfair Display",
        "src": "fonts/PlayfairDisplay-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Playfair Display",
        "src": "fonts/PlayfairDisplay-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Inter",
        "src": "fonts/Inter-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Inter",
        "src": "fonts/Inter-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Inter",
        "src": "fonts/Inter-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      }
    ]
  },
  {
    "id": "madmans-box",
    "name": "Madman's Box",
    "description": "Bigger on the inside, and dressed for the occasion. A mad inventor's time-machine cabin in dark walnut and brass — bakelite knobs, vacuum tubes aglow with amber, a Jacob's ladder crackling cyan between two posts. The room is warm and lamplit; the screens and the energy run cool phosphor-cyan. Banker's-lamp green where things go right. Dark by decree, never by accident. Raleway signage, Lora prose, and a Fira Code console for the dials.",
    "supportsDarkMode": false,
    "previewColors": {
      "background": "hsl(28 32% 7%)",
      "primary": "hsl(40 72% 52%)",
      "secondary": "hsl(30 28% 16%)"
    },
    "fonts": [
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Light.woff2",
        "weight": "300",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Medium.woff2",
        "weight": "500",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Raleway",
        "src": "fonts/Raleway-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-Italic.woff2",
        "weight": "400",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-Medium.woff2",
        "weight": "500",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-MediumItalic.woff2",
        "weight": "500",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-SemiBoldItalic.woff2",
        "weight": "600",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Lora",
        "src": "fonts/Lora-BoldItalic.woff2",
        "weight": "700",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-Italic.woff2",
        "weight": "400",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-Medium.woff2",
        "weight": "500",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-MediumItalic.woff2",
        "weight": "500",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-SemiBoldItalic.woff2",
        "weight": "600",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Mulish",
        "src": "fonts/Mulish-BoldItalic.woff2",
        "weight": "700",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Fira Code",
        "src": "fonts/FiraCode-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Fira Code",
        "src": "fonts/FiraCode-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      }
    ]
  },
  {
    "id": "old-school",
    "name": "Old School",
    "description": "The original Quilltap default theme preserved as a plugin. Warm slate-blue palette with Inter for UI and EB Garamond for branding and long-form content. Professional, clean, and comfortable for extended sessions.",
    "supportsDarkMode": true,
    "previewColors": {
      "background": "hsl(220 20% 97%)",
      "primary": "hsl(220 60% 20%)",
      "secondary": "hsl(220 15% 92%)"
    },
    "fonts": [
      {
        "family": "Inter",
        "src": "fonts/Inter-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Inter",
        "src": "fonts/Inter-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Inter",
        "src": "fonts/Inter-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "EB Garamond",
        "src": "fonts/EBGaramond-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "EB Garamond",
        "src": "fonts/EBGaramond-SemiBold.woff2",
        "weight": "600",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "EB Garamond",
        "src": "fonts/EBGaramond-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      }
    ]
  },
  {
    "id": "rains",
    "name": "Rains",
    "description": "A Claude-inspired warm theme with clean charcoal dark mode, parchment light mode, and orange-amber accents",
    "supportsDarkMode": true,
    "previewColors": {
      "background": "hsl(30 22% 97%)",
      "primary": "hsl(10 53% 53%)",
      "secondary": "hsl(30 11% 94%)"
    },
    "fonts": [
      {
        "family": "Nunito Sans",
        "src": "fonts/NunitoSans-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Nunito Sans",
        "src": "fonts/NunitoSans-Italic.woff2",
        "weight": "400",
        "style": "italic",
        "display": "swap"
      },
      {
        "family": "Fira Code",
        "src": "fonts/FiraCode-Regular.woff2",
        "weight": "400",
        "style": "normal",
        "display": "swap"
      },
      {
        "family": "Fira Code",
        "src": "fonts/FiraCode-Bold.woff2",
        "weight": "700",
        "style": "normal",
        "display": "swap"
      }
    ]
  }
];
