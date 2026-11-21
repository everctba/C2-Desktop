use crate::store::Task;

pub fn pagina(tarefas: &[Task]) -> String {
    let mut itens = String::new();
    for t in tarefas {
        let marca = if t.done { "concluida" } else { "aberta" };
        itens.push_str(&format!(
            "<li data-c2-block=\"task-{}\"><strong>{}</strong> — {}</li>\n",
            t.id, t.title, marca
        ));
    }
    format!(
        r#"<!doctype html>
<html lang="pt-BR">
<head><meta charset="utf-8"><title>Taskline</title></head>
<body>
<header data-c2-block="hero">
  <h1>Taskline</h1>
  <p>Uma lista de tarefas pequena para testar o harness.</p>
</header>
<main>
  <section id="features" data-c2-block="features">
    <h2>Recursos</h2>
    <p>Adicionar, listar e concluir tarefas com persistencia local.</p>
  </section>
  <section id="tasks" data-c2-block="tasks">
    <h2>Tarefas</h2>
    <ul>
{itens}    </ul>
  </section>
  <section id="pricing" data-c2-block="pricing">
    <h2>Plano</h2>
    <p>Gratuito para uso local, sem servidor e sem conta.</p>
  </section>
</main>
</body>
</html>
"#
    )
}
