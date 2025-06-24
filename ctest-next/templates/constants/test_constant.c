{% let c_type_unmapped = Translator::default().translate_type(constant.ty) %}
{% let c_type = generator.map(MapInput::Type(c_type_unmapped, false, false)) %}
{% let ident = generator.map(MapInput::Const(constant)) %}

static const {{ c_type }} __test_const_{{ ident }}_val = {{ ident }};

const {{ c_type }}* __test_const_{{ ident }}(void) {
    return &__test_const_{{ ident }}_val;
}