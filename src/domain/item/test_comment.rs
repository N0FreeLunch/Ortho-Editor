// This is a normal English comment (should pass)

// This contains standard keys: !@#$%^&*()_+=-{}[]|:;"'<>,.?/~` (should pass)

// This contains unicode arrows: A → B, X ≠ Y, ✓ Done (should pass)

// This contains backticked Korean: `방어력` coefficient is applied (should pass)

// `이 주석 전체는 한글이지만 백틱 내부이므로 안전합니다.` (should pass)

// 이 주석은 백틱 바깥에 한글이 있으므로 차단되어야 합니다. (should fail)
