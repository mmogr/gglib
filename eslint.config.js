import js from '@eslint/js';
import tsParser from '@typescript-eslint/parser';
import tseslint from 'typescript-eslint';
import reactPlugin from 'eslint-plugin-react';
import reactHooksPlugin from 'eslint-plugin-react-hooks';
import globals from 'globals';

export default [
  js.configs.recommended,
  {
    files: ['src/**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        ecmaFeatures: {
          jsx: true,
        },
      },
      globals: {
        ...globals.browser,
        ...globals.es2021,
        process: 'readonly',
        __dirname: 'readonly',
      },
    },
    plugins: {
      react: reactPlugin,
      'react-hooks': reactHooksPlugin,
      '@typescript-eslint': tseslint.plugin,
    },
    rules: {
      // TypeScript itself reports undefined identifiers (including DOM/React
      // types) with full type information; the core no-undef rule produces
      // false positives on TS sources and is officially recommended OFF for
      // TypeScript by typescript-eslint.
      'no-undef': 'off',

      // The core rule counts parameter names in TS type/interface signatures
      // as "unused". Use the TS-aware variant instead; underscore-prefixed
      // names are the conventional opt-out for intentionally unused values.
      'no-unused-vars': 'off',
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],

      // React rules
      'react/jsx-uses-react': 'off', // Not needed in React 19
      'react/react-in-jsx-scope': 'off', // Not needed in React 19
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',

      // Phase 5 Architecture Guardrails

      // Forbid legacy CSS classes (Phase 1 cleanup).
      //
      // NOTE: earlier revisions also banned the design-token spacing
      // utilities (p-sm, mb-md, gap-base, …). Those tokens are the
      // project's OWN Tailwind theme (see src/styles/base/variables.css
      // → --spacing-* mapped in src/styles/tailwind.css), used
      // pervasively and intentionally — the ban only survived while the
      // lint script itself was broken. Only the genuinely legacy .btn
      // class remains banned.
      'no-restricted-syntax': [
        'error',
        {
          selector: 'Literal[value=/\\bbtn\\b/]',
          message: 'Use the Button primitive from src/components/ui/Button.tsx instead of legacy .btn classes',
        },
      ],

      // Warn about inline styles (except truly dynamic ones)
      'no-restricted-properties': [
        'warn',
        {
          object: '*',
          property: 'style',
          message: 'Avoid inline styles. Use Tailwind classes or CSS modules. If truly dynamic, add a TODO comment explaining why.',
        },
      ],
    },
  },
  {
    // Allow specific patterns
    files: ['src/**/*.{ts,tsx}'],
    rules: {
      // Allow empty exports for re-export files
      'no-empty-pattern': 'off',
    },
  },
];
