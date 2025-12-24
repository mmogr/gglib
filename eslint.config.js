import js from '@eslint/js';
import tsParser from '@typescript-eslint/parser';
import reactPlugin from 'eslint-plugin-react';
import reactHooksPlugin from 'eslint-plugin-react-hooks';

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
        console: 'readonly',
        process: 'readonly',
        __dirname: 'readonly',
        window: 'readonly',
        document: 'readonly',
        navigator: 'readonly',
        localStorage: 'readonly',
        sessionStorage: 'readonly',
        fetch: 'readonly',
        Headers: 'readonly',
        Request: 'readonly',
        Response: 'readonly',
      },
    },
    plugins: {
      react: reactPlugin,
      'react-hooks': reactHooksPlugin,
    },
    rules: {
      // React rules
      'react/jsx-uses-react': 'off', // Not needed in React 19
      'react/react-in-jsx-scope': 'off', // Not needed in React 19
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',

      // Phase 5 Architecture Guardrails
      
      // Forbid legacy CSS classes (Phase 1 cleanup)
      'no-restricted-syntax': [
        'error',
        {
          selector: 'Literal[value=/\\bm-(0|xs|sm|md|base|lg|xl|2xl|3xl|auto)\\b/]',
          message: 'Use Tailwind margin classes (e.g., m-0, m-2, m-4) instead of custom utility classes',
        },
        {
          selector: 'Literal[value=/\\b(mt|mb|ml|mr|mx|my)-(0|xs|sm|md|base|lg|xl|2xl|3xl|auto)\\b/]',
          message: 'Use Tailwind spacing classes (e.g., mt-2, mb-4) instead of custom utility classes',
        },
        {
          selector: 'Literal[value=/\\bp-(0|xs|sm|md|base|lg|xl|2xl|3xl|auto)\\b/]',
          message: 'Use Tailwind padding classes (e.g., p-0, p-2, p-4) instead of custom utility classes',
        },
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
