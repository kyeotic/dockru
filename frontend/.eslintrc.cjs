module.exports = {
    root: true,
    env: {
        browser: true,
        node: true,
        es2020: true,
    },
    extends: [
        "eslint:recommended",
        "plugin:@typescript-eslint/recommended",
        "plugin:vue/vue3-recommended",
        "prettier",
    ],
    parser: "vue-eslint-parser",
    parserOptions: {
        "parser": "@typescript-eslint/parser",
        ecmaVersion: "latest",
        sourceType: "module",
    },
    plugins: [
        "@typescript-eslint",
        "jsdoc"
    ],
    rules: {
        "yoda": "error",
        "camelcase": [ "warn", {
            "properties": "never",
            "ignoreImports": true
        }],
        "no-unused-vars": [ "warn", {
            "args": "none"
        }],
        "vue/max-attributes-per-line": "off",
        "vue/singleline-html-element-content-newline": "off",
        "vue/html-self-closing": "off",
        "vue/require-component-is": "off",      // not allow is="style" https://github.com/vuejs/eslint-plugin-vue/issues/462#issuecomment-430234675
        "vue/attribute-hyphenation": "off",     // This change noNL to "no-n-l" unexpectedly
        "vue/multi-word-component-names": "off",
        "curly": "error",
        "no-var": "error",
        "no-constant-condition": [ "error", {
            "checkLoops": false,
        }],
        "no-extra-boolean-cast": "off",
        "no-unneeded-ternary": "error",
        "no-empty": [ "error", {
            "allowEmptyCatch": true
        }],
        "no-control-regex": "off",
        "one-var": [ "error", "never" ],
        "@typescript-eslint/ban-ts-comment": "off",
        "@typescript-eslint/no-unused-vars": [ "warn", {
            "args": "none"
        }],
        "prefer-const" : "off",
    },
};
