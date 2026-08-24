/**
 * @file SCSS grammar for tree-sitter
 * @author Amaan Qureshi <amaanq12@gmail.com>
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const CSS = require('tree-sitter-css/grammar');

module.exports = grammar(CSS, {
  name: 'scss',

  externals: ($, original) => original.concat([
    $._concat,
    $._map_open,
    $._modulo,
  ]),

  // A key built from an expression and the expression itself are the same text read two
  // ways, and which one it is depends on the colon that has not been reached yet.
  conflicts: ($, previous) => previous.concat([
    [$.binary_expression, $.map_entry],
    [$.feature_query, $.map_entry],
    [$._query, $._concatenated_identifier],
  ]),

  rules: {
    _top_level_item: ($, original) => choice(
      original,
      $.postcss_statement,
      $.use_statement,
      $.forward_statement,
      $.mixin_statement,
      $.include_statement,
      $.function_statement,
      $.return_statement,
      $.extend_statement,
      $.error_statement,
      $.warn_statement,
      $.debug_statement,
      $.at_root_statement,
      $.if_statement,
      $.each_statement,
      $.for_statement,
      $.while_statement,
    ),

    _block_item: ($, original) => choice(
      original,
      $.mixin_statement,
      $.include_statement,
      $.function_statement,
      $.return_statement,
      $.extend_statement,
      $.error_statement,
      $.warn_statement,
      $.debug_statement,
      $.at_root_statement,
      $.if_statement,
      $.each_statement,
      $.for_statement,
      $.while_statement,
    ),

    // Selectors

    selectors: $ => sep1(',', seq(
      optional(choice('>', '+', '~')),
      $._selector,
    )),

    _selector: ($, original) => choice(
      original,
      alias($._concatenated_identifier, $.tag_name),
      $.placeholder,
    ),

    class_selector: $ => prec(1, seq(
      optional($._selector),
      choice('.', $.nesting_selector),
      alias(choice($.identifier, $._concatenated_identifier), $.class_name),
    )),

    pseudo_class_selector: $ => seq(
      optional($._selector),
      alias($._pseudo_class_selector_colon, ':'),
      alias(choice($.identifier, $._concatenated_identifier), $.class_name),
      optional(seq(
        alias($.pseudo_class_arguments, $.arguments),
        repeat(seq($._concat, $.interpolation)),
      )),
    ),

    // Declarations

    declaration: $ => seq(
      alias(
        choice($.identifier, $.variable, $._concatenated_identifier),
        $.property_name,
      ),
      ':',
      $._value,
      repeat(seq(optional(','), $._value)),
      repeat($._declaration_flag),
      ';',
    ),

    last_declaration: $ => prec(1, seq(
      alias(
        choice($.identifier, $.variable, $._concatenated_identifier),
        $.property_name,
      ),
      ':',
      $._value,
      repeat(seq(optional(','), $._value)),
      repeat($._declaration_flag),
    )),

    _declaration_flag: $ => choice($.important, $.default_flag, $.global_flag),

    // Media queries

    _query: ($, original) => choice(
      original,
      $.interpolation,
    ),

    // Property Values

    _value: ($, original) => choice(
      original,
      prec(-1, choice(
        $.nesting_selector,
        $._concatenated_identifier,
        $.list_value,
        $.map_value,
      )),
      $.url_value,
      $.unary_expression,
      $.negated_variable,
      $.variable,
    ),

    use_statement: $ => seq(
      '@use',
      $._value,
      optional(seq('as', choice('*', field('alias', $.identifier)))),
      optional(seq('with', field('configuration', $.map_value))),
      ';',
    ),

    forward_statement: $ => seq(
      '@forward',
      $._value,
      optional(seq('as', field('prefix', $.forward_prefix))),
      optional(seq(
        choice('show', 'hide'),
        sep1(',', choice($.identifier, $.variable)),
      )),
      optional(seq('with', field('configuration', $.map_value))),
      ';',
    ),

    forward_prefix: $ => seq($.identifier, token.immediate('*')),

    mixin_statement: $ => seq(
      '@mixin',
      field('name', $.identifier),
      optional($.parameters),
      $.block,
    ),

    include_statement: $ => seq(
      '@include',
      choice($.identifier, $.namespaced_name),
      optional(alias($._include_arguments, $.arguments)),
      choice($.block, ';'),
    ),

    _include_arguments: $ => seq(
      choice('(', alias($._map_open, '(')),
      optional(seq(sep1(',', alias($._include_argument, $.argument)), optional(','))),
      ')',
    ),

    _include_argument: $ => seq(
      optional(seq(field('name', $.variable), ':')),
      field('value', repeat1($._value)),
      optional('...'),
    ),

    arguments: $ => seq(
      token.immediate('('),
      optional(seq(sep1(choice(',', ';'), seq(
        optional(seq(field('name', $.variable), ':')),
        repeat1($._value),
        optional('...'),
      )), optional(','))),
      ')',
    ),

    function_statement: $ => seq(
      '@function',
      field('name', $.identifier),
      optional($.parameters),
      $.block,
    ),

    parameters: $ => seq('(', optional(sep1(',', $.parameter)), ')'),

    parameter: $ => seq(
      $.variable,
      optional('...'),
      optional(seq(
        ':',
        field('default', repeat1($._value)),
      )),
    ),

    return_statement: $ => seq('@return', sep1(',', repeat1($._value)), ';'),

    extend_statement: $ => seq('@extend', choice($._value, $.class_selector, $.placeholder), ';'),

    error_statement: $ => seq('@error', $._value, ';'),

    warn_statement: $ => seq('@warn', $._value, ';'),

    debug_statement: $ => seq('@debug', $._value, ';'),

    at_root_statement: $ => seq('@at-root', $._value, $.block),

    if_statement: $ => seq(
      '@if',
      field('condition', $._value),
      $.block,
      repeat($.else_if_clause),
      optional($.else_clause),
    ),

    else_if_clause: $ => seq(
      '@else',
      'if',
      field('condition', $._value),
      $.block,
    ),

    else_clause: $ => seq('@else', $.block),

    each_statement: $ => seq(
      '@each',
      optional(seq(field('key', $.variable), ',')),
      field('value', $.variable),
      'in',
      $._value,
      $.block,
    ),

    for_statement: $ => seq(
      '@for',
      $.variable,
      'from',
      field('from', $._value),
      'through',
      field('through', $._value),
      $.block,
    ),

    while_statement: $ => seq('@while', $._value, $.block),

    call_expression: $ => seq(
      alias(choice($.identifier, $.plain_value), $.function_name),
      $.arguments,
    ),

    binary_expression: $ => prec.left(seq(
      $._value,
      choice(
        '+', '-', '*', '/', alias($._modulo, '%'),
        '==', '<', '>', '!=', '<=', '>=', 'and', 'or',
      ),
      $._value,
    )),

    unary_expression: $ => prec.right(seq('not', $._value)),

    list_value: $ => seq(
      '(',
      optional(choice(
        seq(sep2(',', repeat1($._value)), optional(',')),
        seq(repeat1($._value), ','),
      )),
      ')',
    ),

    map_value: $ => seq(
      alias($._map_open, '('),
      sep1(',', $.map_entry),
      optional(','),
      ')',
    ),

    map_entry: $ => seq(
      field('key', seq(
        choice(
          $.identifier,
          $.variable,
          $.string_value,
          $.integer_value,
          $.float_value,
          $._concatenated_identifier,
        ),
        repeat(seq(choice('+', '-', '*', '/'), $._value)),
      )),
      ':',
      field('value', repeat1($._value)),
      repeat($._declaration_flag),
    ),

    default_flag: _ => '!default',

    global_flag: _ => '!global',

    interpolation: $ => seq('#{', repeat1($._value), '}'),

    namespaced_name: $ => seq(
      field('namespace', $.identifier),
      token.immediate('.'),
      field('name', alias(token.immediate(/[a-zA-Z_-][a-zA-Z0-9_-]*/), $.identifier)),
    ),

    placeholder: $ => seq('%', choice($.identifier, $._concatenated_identifier)),

    keyframes_statement: $ => seq(
      choice(
        '@keyframes',
        alias(/@[-a-z]+keyframes/, $.at_keyword),
      ),
      alias(choice($.identifier, $._concatenated_identifier), $.keyframes_name),
      $.keyframe_block_list,
    ),

    keyframe_block: $ => seq(
      sep1(',', choice($.from, $.to, $.integer_value, $.float_value)),
      $.block,
    ),

    attribute_selector: $ => seq(
      optional($._selector),
      '[',
      alias(
        choice($.identifier, $._concatenated_identifier, $.namespace_selector),
        $.attribute_name,
      ),
      optional(seq(
        choice('=', '~=', '^=', '|=', '*=', '$='),
        $._value,
      )),
      ']',
    ),

    at_rule: $ => seq(
      $.at_keyword,
      optional(sep1(',', repeat1($._query))),
      choice(';', $.block),
    ),

    feature_query: $ => seq(
      choice('(', alias($._map_open, '(')),
      alias(choice($.identifier, $._concatenated_identifier), $.feature_name),
      ':',
      repeat1($._value),
      ')',
    ),

    pseudo_element_selector: $ => seq(
      optional($._selector),
      '::',
      alias(choice($.identifier, $._concatenated_identifier), $.tag_name),
      optional(alias($.pseudo_element_arguments, $.arguments)),
    ),

    pseudo_class_arguments: $ => seq(
      token.immediate('('),
      optional(sep1(',', choice(
        $.nth_expression,
        seq(optional(choice('>', '+', '~')), $._selector),
        repeat1($._value),
      ))),
      ')',
    ),

    // `:nth-child(n + 3)`. The `n` alone reads as a tag name, and the count that
    // follows it does not, so the whole step is one token.
    nth_expression: _ => token(seq(
      optional('-'),
      'n',
      /\s*/,
      /[+-]/,
      /\s*/,
      /\d+/,
    )),

    _concatenated_identifier: $ => choice(
      seq(
        $.identifier,
        repeat1(seq(
          $._concat,
          choice($.interpolation, alias($._name_part, $.identifier)),
        )),
      ),
      seq(
        $.interpolation,
        repeat(seq(
          $._concat,
          choice($.interpolation, alias($._name_part, $.identifier)),
        )),
      ),
    ),

    url_value: _ => token(seq(
      /[uU][rR][lL]\(/,
      /[^)'"\s]*[,;!][^)'"\s]*/,
      ')',
    )),

    _name_part: _ => token.immediate(/([a-zA-Z0-9_-]|\\.)+/),

    identifier: _ => /(--|-?[a-zA-Z_]|\\.)([a-zA-Z0-9-_]|\\.)*/,

    variable: _ => token(prec(1, /([a-zA-Z_]+\.)?\$[a-zA-Z-_][a-zA-Z0-9-_]*/)),

    negated_variable: _ => token(seq('-', /([a-zA-Z_]+\.)?\$[a-zA-Z-_][a-zA-Z0-9-_]*/)),

    plain_value: _ => token(seq(
      repeat(choice(
        /[-_]/,
        /\/[^\*\s,;!{}()\[\]]/,
      )),
      choice(/[a-zA-Z]/, /\\./),
      repeat(choice(
        /[^#/\s,;!{}()\[\]]/,
        /#[^{]/,
        /\/[^\*\s,;!{}()\[\]]/,
      )),
    )),
  },
});

/**
 * Creates a rule to match one or more of the rules separated by `separator`
 *
 * @param {RuleOrLiteral} separator
 *
 * @param {RuleOrLiteral} rule
 *
 * @return {SeqRule}
 *
 */
function sep1(separator, rule) {
  return seq(rule, repeat(seq(separator, rule)));
}

/**
 * Creates a rule to match two or more of the rules separated by `separator`
 *
 * @param {RuleOrLiteral} separator
 *
 * @param {RuleOrLiteral} rules
 *
 * @return {SeqRule}
 */
function sep2(separator, rules) {
  return seq(rules, repeat1(seq(separator, rules)));
}
