{% let c_type_unmapped = Translator::default().translate_type(constant.ty) %}
{% let ident = generator.map(MapInput::Const(constant)) %}
{% let c_type = generator.map(MapInput::Type(c_type_unmapped, ffi_items.contains_struct(ident), ffi_items.contains_union(ident))) %}

static const {{ c_type }} __test_const_{{ ident }}_val = {{ ident }};

const {{ c_type }}* __test_const_{{ ident }}(void) {
    return &__test_const_{{ ident }}_val;
}