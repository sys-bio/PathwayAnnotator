//! The user interface.

use std::rc::Rc;
use std::time::Duration;

use annot_engine::{Confidence, Role};
use annot_model::Element;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::browser;
use crate::session::{
    Counts, Session, Status, creator_name, qualifier_choices, qualifier_key, readable_date, sbo_choices,
};

type SessionSignal = RwSignal<Option<Session>, LocalStorage>;

/// The example model offered on the start screen.
const EXAMPLE: &str = include_str!("../assets/glycolysis-example.ant");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Molecules,
    Reactions,
    Compartments,
    Model,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Review,
    Done,
    Nothing,
}

impl Filter {
    fn keeps(self, s: &Status) -> bool {
        match self {
            Filter::All => true,
            Filter::Review => matches!(s, Status::Suggested { .. }),
            Filter::Done => matches!(s, Status::Identified { .. } | Status::AnnotatedElsewhere { .. }),
            Filter::Nothing => matches!(s, Status::Unknown),
        }
    }
}

/// Shared by every component.
#[derive(Clone, Copy)]
struct Ctx {
    session: SessionSignal,
    message: RwSignal<Option<String>>,
    tab: RwSignal<Tab>,
    filter: RwSignal<Filter>,
    selected: RwSignal<Option<Element>>,
    /// The molecule chip whose card is pinned open: (reaction, position).
    pinned: RwSignal<Option<(Element, usize)>>,
    /// The reaction to go back to after jumping from one of its molecule
    /// chips to the Molecules list.
    return_to: RwSignal<Option<Element>>,
    /// Whose proteins UniProt searches cover (an NCBI taxon id), `None`
    /// for any organism. Starts as the model's organism, if it says.
    organism: RwSignal<Option<u32>>,
    /// The row the keyboard is on ([`on_key`]); clicking a row moves it
    /// there too.
    cursor: RwSignal<Option<Element>>,
    /// Whether the keyboard shortcuts panel is showing.
    help: RwSignal<bool>,
    /// Whether the About panel (data sources and licenses) is showing.
    about: RwSignal<bool>,
    /// Whether the Help page (how to use the annotator) is showing.
    guide: RwSignal<bool>,
    /// The knowledge pack's header: when it was built, and its sources
    /// with their licenses and the notices they require.
    pack_header: RwSignal<Option<kpack::Header>>,
    /// The model's SBML as last opened from or saved to a file.
    saved: RwSignal<Option<String>>,
    /// Whether the open model has changes not saved to a file.
    unsaved: RwSignal<bool>,
}

/// Opens an element in its tab and scrolls it into view.
fn reveal(ctx: Ctx, el: Element) {
    ctx.tab.set(match el {
        Element::Reaction(_) => Tab::Reactions,
        Element::Compartment(_) => Tab::Compartments,
        _ => Tab::Molecules,
    });
    ctx.filter.set(Filter::All);
    ctx.selected.set(Some(el));
    set_timeout(
        move || {
            if let Some(e) = document().get_element_by_id(&dom_id(el)) {
                e.scroll_into_view();
            }
        },
        Duration::from_millis(50),
    );
}

fn dom_id(el: Element) -> String {
    match el {
        Element::Species(i) => format!("species-{i}"),
        Element::Reaction(i) => format!("reaction-{i}"),
        Element::Compartment(i) => format!("compartment-{i}"),
        Element::Model => "model".into(),
    }
}

fn level_text(level: Confidence) -> &'static str {
    match level {
        Confidence::High => "high confidence",
        Confidence::Likely => "likely",
        Confidence::Uncertain => "uncertain",
    }
}

/// A suggestion's name as a link to its database entry (new tab), so people
/// can check it themselves; plain bold text if there is nowhere to go.
/// Clicking it does not open or close the row it is in.
fn check_link(label: String, url: Option<String>) -> AnyView {
    match url {
        Some(url) => view! {
            <a
                class="check"
                href=url
                target="_blank"
                rel="noopener noreferrer"
                title="Check this in its database (opens a new tab)"
                on:click=|ev: web_sys::MouseEvent| ev.stop_propagation()
            >
                {label}
                <svg class="ext" viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
                    <path d="M5 2H2v8h8V7M7 1h4v4M11 1L5.5 6.5" fill="none" stroke="currentColor" stroke-width="1.3"/>
                </svg>
            </a>
        }
            .into_any(),
        None => view! { <b>{label}</b> }.into_any(),
    }
}

#[component]
pub fn App() -> impl IntoView {
    let pack: RwSignal<Option<Rc<kpack::Pack>>, LocalStorage> = RwSignal::new_local(None);
    let pack_error = RwSignal::new(None::<String>);
    let ctx = Ctx {
        session: RwSignal::new_local(None),
        message: RwSignal::new(None),
        tab: RwSignal::new(Tab::Molecules),
        filter: RwSignal::new(Filter::All),
        selected: RwSignal::new(None),
        pinned: RwSignal::new(None),
        return_to: RwSignal::new(None),
        organism: RwSignal::new(None),
        cursor: RwSignal::new(None),
        help: RwSignal::new(false),
        about: RwSignal::new(false),
        guide: RwSignal::new(false),
        pack_header: RwSignal::new(None),
        saved: RwSignal::new(None),
        unsaved: RwSignal::new(false),
    };
    provide_context(ctx);
    // Lives as long as the page: the app never unmounts.
    let _keys = window_event_listener(leptos::ev::keydown, move |ev| on_key(ctx, ev));
    // Reloading or closing the page with unsaved changes asks first.
    let _leave = window_event_listener(leptos::ev::beforeunload, move |ev| {
        if ctx.unsaved.get_untracked() {
            ev.prevent_default();
            ev.set_return_value("You have unsaved changes.");
        }
    });

    // Tracks whether the open model differs from the file it came from
    // or was last saved to.
    Effect::new(move |_| {
        let sbml = ctx.session.with(|s| s.as_ref().map(|s| s.ws.to_sbml()));
        ctx.unsaved.set(sbml.is_some_and(|t| ctx.saved.with(|saved| saved.as_deref() != Some(t.as_str()))));
    });

    spawn_local(async move {
        match browser::fetch_pack("kpack.bin").await {
            Ok(p) => {
                ctx.pack_header.set(Some(p.header.clone()));
                pack.set(Some(Rc::new(p)));
            }
            Err(e) => pack_error.set(Some(e)),
        }
    });

    let open = move |text: String, name: String| {
        let Some(p) = pack.get_untracked() else {
            ctx.message.set(Some("The knowledge base is still loading; try again in a moment.".into()));
            return;
        };
        match annot_model::load(&text) {
            Ok(ws) => {
                ctx.saved.set(Some(ws.to_sbml()));
                ctx.organism.set(ws.model_taxon());
                let mut session = Session::new(p, ws, name);
                session.remember_online(browser::remembered_names());
                ctx.session.set(Some(session));
                ctx.cursor.set(None);
                ctx.selected.set(None);
                ctx.filter.set(Filter::All);
                ctx.message.set(None);
            }
            Err(e) => ctx.message.set(Some(format!("Could not open {name}: {e}."))),
        }
    };

    let pack_ready = Memo::new(move |_| pack.with(|p| p.is_some()));
    let has_session = Memo::new(move |_| ctx.session.with(|s| s.is_some()));

    view! {
        <Header/>
        <main>
            {move || {
                if let Some(e) = pack_error.get() {
                    view! {
                        <p class="error">
                            "The knowledge base could not be loaded (" {e} "). Reload the page to try again."
                        </p>
                    }
                        .into_any()
                } else if !pack_ready.get() {
                    view! { <p class="loading">"Loading the knowledge base..."</p> }.into_any()
                } else if !has_session.get() {
                    view! { <Start open=open/> }.into_any()
                } else {
                    view! {
                        <Intro/>
                        <Overview/>
                        <Tabs/>
                    }
                        .into_any()
                }
            }}
            <Message/>
            <Shortcuts/>
            <About/>
            <Help/>
        </main>
    }
}

#[component]
fn Header() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let file = move || s.with(|s| s.as_ref().map(|s| s.file_name.clone()));
    let can_undo = move || s.with(|s| s.as_ref().is_some_and(|s| s.ws.can_undo()));
    let can_redo = move || s.with(|s| s.as_ref().is_some_and(|s| s.ws.can_redo()));
    let save = move |_| {
        let out = s.with_untracked(|s| s.as_ref().map(|s| (s.export_name(), s.ws.to_sbml())));
        if let Some((name, text)) = out {
            match browser::save_file(&name, &text) {
                Ok(()) => {
                    ctx.saved.set(Some(text));
                    ctx.message.set(Some(format!("Saved {name}.")));
                }
                Err(e) => ctx.message.set(Some(format!("Could not save: {e}."))),
            }
        }
    };
    view! {
        <header class="top">
            <h1>"PathwayAnnotator"</h1>
            <span class="file">{file}</span>
            <button
                class="link shortcuts-button"
                title="Keyboard shortcuts (press ?)"
                aria-expanded=move || ctx.help.get().to_string()
                on:click=move |_| ctx.help.update(|h| *h = !*h)
            >
                "Shortcuts " <kbd>"?"</kbd>
            </button>
            <button
                class="link"
                title="How to use the annotator"
                aria-expanded=move || ctx.guide.get().to_string()
                on:click=move |_| ctx.guide.update(|g| *g = !*g)
            >
                "Help"
            </button>
            <button
                class="link"
                title="What this is, where its data comes from, and the data licenses"
                aria-expanded=move || ctx.about.get().to_string()
                on:click=move |_| ctx.about.update(|a| *a = !*a)
            >
                "About"
            </button>
            <Show when=move || file().is_some()>
                <div class="actions">
                    <button on:click=move |_| s.update(|s| if let Some(s) = s { s.undo() }) disabled=move || !can_undo()>
                        "Undo"
                    </button>
                    <button on:click=move |_| s.update(|s| if let Some(s) = s { s.redo() }) disabled=move || !can_redo()>
                        "Redo"
                    </button>
                    <button class="primary" on:click=save>"Save annotated SBML"</button>
                    <button on:click=move |_| s.set(None)>"Close"</button>
                </div>
            </Show>
        </header>
    }
}

#[component]
fn Start<F>(open: F) -> impl IntoView
where
    F: Fn(String, String) + Copy + Send + Sync + 'static,
{
    let ctx = expect_context::<Ctx>();
    let dragging = RwSignal::new(false);
    let pasted = RwSignal::new(String::new());
    // SBML is XML and starts with '<'; anything else is read as Antimony.
    let open_pasted = move || {
        let text = pasted.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        let name = if text.trim_start().starts_with('<') { "pasted-model.xml" } else { "pasted-model.ant" };
        open(text, name.into());
    };
    let read = move |file: web_sys::File| {
        spawn_local(async move {
            match browser::read_file(&file).await {
                Ok(text) => open(text, file.name()),
                Err(e) => ctx.message.set(Some(format!("Could not read {}: {e}.", file.name()))),
            }
        });
    };
    view! {
        <section class="start">
            <div class="purpose">
                <h2>"Say what your model is made of"</h2>
                <p>
                    "In a model, a molecule called " <code>"G6P"</code> " or " <code>"S1"</code>
                    " means little to anyone but its author. Annotation links each molecule, reaction and "
                    "compartment to the database entry it stands for (ChEBI, Rhea, KEGG, GO and others), so that "
                    "people and software can tell what it is. Annotated models are easier to find, check, reuse "
                    "and combine; the curated models in BioModels are annotated this way."
                </p>
                <p>"Doing this by hand is slow. PathwayAnnotator helps:"</p>
                <ul>
                    <li>
                        <b>"It suggests."</b>
                        " For each molecule, metabolic reaction and compartment it proposes a database entry, with a confidence "
                        "level, using a knowledge base built from BiGG, ChEBI and BioModels."
                    </li>
                    <li>
                        <b>"You decide."</b>
                        " Every suggestion shows its evidence and links to the database entry; nothing is added "
                        "to the model until you accept it."
                    </li>
                    <li>
                        <b>"Your model stays private."</b>
                        " Everything runs in your browser; the model is not uploaded. Online searches happen only "
                        "when you ask for one."
                    </li>
                </ul>
                <p>
                    "When you are done, save the model as SBML with the annotations included."
                </p>
                <p class="purpose-links">
                    <button class="link" on:click=move |_| ctx.guide.set(true)>"How it works"</button>
                    " | "
                    <button class="link" on:click=move |_| ctx.about.set(true)>"About and data sources"</button>
                </p>
            </div>
            <h2>"Annotate a model"</h2>
            <p>"Open an SBML or Antimony model to start."</p>
            <label
                class="drop"
                class:dragging=move || dragging.get()
                on:dragover=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    dragging.set(true);
                }
                on:dragleave=move |_| dragging.set(false)
                on:drop=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    dragging.set(false);
                    if let Some(file) = ev.data_transfer().and_then(|d| d.files()).and_then(|f| f.get(0)) {
                        read(file);
                    }
                }
            >
                <input
                    type="file"
                    accept=".xml,.sbml,.ant,.txt"
                    on:change=move |ev| {
                        let input: web_sys::HtmlInputElement = event_target(&ev);
                        if let Some(file) = input.files().and_then(|f| f.get(0)) {
                            read(file);
                        }
                    }
                />
                <span class="drop-text">"Drop a model file here, or click to choose one"</span>
            </label>
            <div class="paste">
                <label for="paste-model">"Or paste SBML or Antimony text:"</label>
                <textarea
                    id="paste-model"
                    rows="8"
                    spellcheck="false"
                    placeholder="model example\n  J1: S1 -> S2; k1*S1\n  S1 = 10; k1 = 0.1\nend"
                    prop:value=move || pasted.get()
                    on:input=move |ev| pasted.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        if ev.key() == "Enter" && (ev.ctrl_key() || ev.meta_key()) {
                            ev.prevent_default();
                            open_pasted();
                        }
                    }
                ></textarea>
                <div class="paste-actions">
                    <button
                        class="primary"
                        disabled=move || pasted.with(|t| t.trim().is_empty())
                        on:click=move |_| open_pasted()
                    >
                        "Open pasted model"
                    </button>
                    <span class="muted">"Ctrl+Enter also opens it."</span>
                </div>
            </div>
            <p class="or">
                "No model at hand? "
                <button class="link" on:click=move |_| open(EXAMPLE.to_string(), "glycolysis-example.ant".into())>
                    "Try the glycolysis example"
                </button>
            </p>
        </section>
    }
}

#[component]
fn Overview() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let counts = move || {
        s.with(|s| {
            s.as_ref()
                .map(|s| (s.counts(&s.species()), s.counts(&s.reactions())))
                .unwrap_or_default()
        })
    };
    let high = move || {
        let (a, b) = counts();
        a.high + b.high
    };
    let accept_high = move |_| {
        let mut n = 0;
        s.update(|s| {
            if let Some(s) = s {
                n = s.accept_all_high();
            }
        });
        ctx.message.set(Some(format!("Accepted {n} high-confidence suggestions. Undo reverts all of them in one step.")));
    };
    view! {
        <section class="overview">
            <Progress title="Molecules" counts=Signal::derive(move || counts().0)/>
            <Progress title="Reactions" counts=Signal::derive(move || counts().1)/>
            <div class="bulk">
                <button class="primary" disabled=move || high() == 0 on:click=accept_high>
                    {move || format!("Accept all high-confidence suggestions ({})", high())}
                </button>
            </div>
        </section>
    }
}

#[component]
fn Progress(title: &'static str, counts: Signal<Counts>) -> impl IntoView {
    let pct = move |n: usize| {
        let c = counts.get();
        if c.total == 0 { 0.0 } else { 100.0 * n as f64 / c.total as f64 }
    };
    view! {
        <div class="progress">
            <div class="progress-head">
                <b>{title}</b>
                <span>{move || {
                    let c = counts.get();
                    format!("{} of {} identified", c.done, c.total)
                }}</span>
            </div>
            <div class="bar" role="img" aria-label=move || {
                let c = counts.get();
                format!("{} identified, {} high, {} likely, {} uncertain, {} with no suggestion", c.done, c.high, c.likely, c.uncertain, c.unknown)
            }>
                <span class="seg done" style=move || format!("width:{}%", pct(counts.get().done))></span>
                <span class="seg high" style=move || format!("width:{}%", pct(counts.get().high))></span>
                <span class="seg likely" style=move || format!("width:{}%", pct(counts.get().likely))></span>
                <span class="seg uncertain" style=move || format!("width:{}%", pct(counts.get().uncertain))></span>
            </div>
            <div class="legend">
                <span><i class="dot done"></i>{move || format!("identified {}", counts.get().done)}</span>
                <span><i class="dot high"></i>{move || format!("high {}", counts.get().high)}</span>
                <span><i class="dot likely"></i>{move || format!("likely {}", counts.get().likely)}</span>
                <span><i class="dot uncertain"></i>{move || format!("uncertain {}", counts.get().uncertain)}</span>
                <span><i class="dot unknown"></i>{move || format!("no suggestion {}", counts.get().unknown)}</span>
            </div>
        </div>
    }
}

#[component]
fn Tabs() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let tab_button = move |t: Tab, label: &'static str| {
        view! {
            <button
                role="tab"
                class="tab"
                class:active=move || ctx.tab.get() == t
                aria-selected=move || (ctx.tab.get() == t).to_string()
                on:click=move |_| {
                    ctx.tab.set(t);
                    ctx.cursor.set(None);
                }
            >
                {label}
            </button>
        }
    };
    let filter_button = move |f: Filter, label: &'static str| {
        view! {
            <button class="chip-filter" class:active=move || ctx.filter.get() == f on:click=move |_| ctx.filter.set(f)>
                {label}
            </button>
        }
    };
    view! {
        <nav class="tabs" role="tablist">
            {tab_button(Tab::Molecules, "Molecules")}
            {tab_button(Tab::Reactions, "Reactions")}
            {tab_button(Tab::Compartments, "Compartments")}
            {tab_button(Tab::Model, "Model")}
            <span class="filters" class:hidden=move || ctx.tab.get() == Tab::Model>
                {filter_button(Filter::All, "All")}
                {filter_button(Filter::Review, "To review")}
                {filter_button(Filter::Done, "Identified")}
                {filter_button(Filter::Nothing, "No suggestion")}
            </span>
        </nav>
        <Show when=move || ctx.tab.get() == Tab::Model fallback=|| view! { <ItemList/> }>
            <ModelPanel/>
        </Show>
    }
}

/// The rows of a tab under a filter, in order.
fn rows_of(s: &Session, tab: Tab, filter: Filter) -> Vec<Element> {
    let all = match tab {
        Tab::Molecules => s.species(),
        Tab::Reactions => s.reactions(),
        Tab::Compartments => s.compartments(),
        Tab::Model => Vec::new(),
    };
    all.into_iter().filter(|el| filter.keeps(&s.status(*el))).collect()
}

#[component]
fn ItemList() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let items = move || {
        let (tab, filter) = (ctx.tab.get(), ctx.filter.get());
        ctx.session.with(|s| s.as_ref().map(|s| rows_of(s, tab, filter)).unwrap_or_default())
    };
    // After jumping from a reaction's molecule chip: the way back.
    let back = move || {
        let r = ctx.return_to.get()?;
        if ctx.tab.get() != Tab::Molecules {
            return None;
        }
        let name = ctx.session.with(|s| s.as_ref().map(|s| s.display_name(r)))?;
        Some(view! {
            <div class="back">
                <button on:click=move |_| {
                    ctx.return_to.set(None);
                    reveal(ctx, r);
                }>
                    <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                        <path d="M7.5 2.5L4 6l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5"/>
                    </svg>
                    {format!(" Back to reaction {name}")}
                </button>
            </div>
        })
    };
    view! {
        {back}
        <ul class="items">
            <For each=items key=|el| *el children=move |el| view! { <Item el=el/> }/>
        </ul>
        <Show when=move || items().is_empty()>
            <p class="empty">"Nothing here with this filter."</p>
        </Show>
    }
}

#[component]
fn Item(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let name = move || s.with(|s| s.as_ref().map(|s| s.display_name(el)).unwrap_or_default());
    let id = move || {
        s.with(|s| {
            s.as_ref()
                .map(|s| s.ws.element_id(el).to_string())
                .filter(|id| *id != name())
                .unwrap_or_default()
        })
    };
    let status = move || s.with(|s| s.as_ref().map(|s| s.status(el)).unwrap_or(Status::Unknown));
    let open = move || ctx.selected.get() == Some(el);
    let toggle = move |_| {
        ctx.cursor.set(Some(el));
        ctx.selected.update(|sel| *sel = if *sel == Some(el) { None } else { Some(el) });
    };
    // A compartment's suggestion is a GO term from the pack's table of
    // compartment names, not a pack entry.
    let compartment = matches!(el, Element::Compartment(_));
    let accept_first = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            let Some(s) = s else { return };
            if compartment {
                let msg = s.accept_compartment(el);
                ctx.message.set(Some(msg));
            } else if let Some(c) = s.candidates(el).first() {
                let msg = s.accept(el, c.target);
                ctx.message.set(Some(msg));
            }
        });
    };
    let reject_first = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            let Some(s) = s else { return };
            if compartment {
                let msg = s.reject_compartment(el);
                ctx.message.set(Some(msg));
            } else if let Some(c) = s.candidates(el).first() {
                let msg = s.reject(el, c.target);
                ctx.message.set(Some(msg));
            }
        });
    };
    // Where to check the suggestion, or the current identification.
    let suggestion_link = move || {
        s.with(|s| {
            s.as_ref().and_then(|s| {
                if compartment {
                    return s.compartment_suggestion(el).map(|(uri, _)| uri);
                }
                s.candidates(el).first().and_then(|c| s.main_link(c.target))
            })
        })
    };
    let identity_link = move || s.with(|s| s.as_ref().and_then(|s| s.ws.identity_uris(el).into_iter().next()));
    // The most familiar other name of the suggestion: "D-glucopyranose,
    // also glucose".
    let suggestion_aka = move || {
        s.with(|s| {
            s.as_ref().and_then(|s| {
                s.candidates(el)
                    .first()
                    .and_then(|c| s.synonyms(c.target).into_iter().next())
            })
        })
    };
    let what = move || match status() {
        Status::Identified { label } => view! {
            <span class="what">"Identified as " {check_link(label, identity_link())}</span>
        }
            .into_any(),
        Status::AnnotatedElsewhere { label: Some(label) } => {
            view! { <span class="what">"Identified as " {check_link(label, identity_link())}</span> }.into_any()
        }
        Status::AnnotatedElsewhere { label: None } => {
            view! { <span class="what">"Annotated (with something outside our database)"</span> }.into_any()
        }
        Status::Suggested { label, level } => view! {
            <span class="what">
                {check_link(label, suggestion_link())}
                {suggestion_aka().map(|a| view! { <span class="aka">"also " {a}</span> })}
                <span class=format!("badge {}", Status::Suggested { label: String::new(), level }.class())>
                    {level_text(level)}
                </span>
            </span>
            <button class="accept" on:click=accept_first>"Accept"</button>
            <button class="reject" title="Not this: do not suggest it again" on:click=reject_first>
                "Reject"
            </button>
        }
            .into_any(),
        Status::Unknown => {
            view! { <span class="what muted">"No suggestion: search other databases in Details"</span> }.into_any()
        }
    };
    view! {
        <li class="item" class:open=open class:cursor=move || ctx.cursor.get() == Some(el) id=dom_id(el)>
            <div class="row" on:click=toggle>
                <span class=move || format!("dot {}", status().class())></span>
                <span class="name">{name}</span>
                <span class="id">{id}</span>
                {what}
                // An explicit control: the whole row opens too, but nothing
                // else says so.
                <button
                    class="disclose"
                    aria-expanded=move || open().to_string()
                    title="Show the suggestions, their evidence and the current annotation"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.stop_propagation();
                        ctx.selected.update(|sel| *sel = if *sel == Some(el) { None } else { Some(el) });
                    }
                >
                    {move || if open() { "Hide" } else { "Details" }}
                    <svg class="chevron" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                        <path d="M3 4.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.5"/>
                    </svg>
                </button>
            </div>
            {matches!(el, Element::Reaction(_)).then(|| view! { <Equation el=el/> })}
            <Show when=open>
                <Details el=el/>
            </Show>
        </li>
    }
}

/// A reaction as "A + B -> C", each molecule a chip coloured by its status.
/// Hovering (or tabbing to) a chip shows a card about the molecule; clicking
/// pins the card open. The card can accept the molecule's suggestion in
/// place, so checking a reaction's molecules never leaves the reaction.
#[component]
fn Equation(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let Element::Reaction(ri) = el else { return ().into_any() };
    // (position in the reaction, species, label, status class)
    let side = move |role: Role| {
        ctx.session.with(|s| {
            let Some(s) = s else { return Vec::new() };
            s.model.reactions[ri]
                .participants
                .iter()
                .enumerate()
                .filter(|(_, p)| p.role == role)
                .map(|(k, p)| {
                    let sp = Element::Species(p.species);
                    let n = if p.stoichiometry == 1.0 { String::new() } else { format!("{} ", p.stoichiometry) };
                    (k, sp, format!("{n}{}", s.display_name(sp)), s.status(sp).class())
                })
                .collect::<Vec<_>>()
        })
    };
    let join = move |items: Vec<(usize, Element, String, &'static str)>| {
        let n = items.len();
        items
            .into_iter()
            .enumerate()
            .map(|(i, (k, sp, label, class))| {
                view! {
                    <Chip reaction=el position=k species=sp label=label class=class/>
                    {(i + 1 < n).then_some(" + ")}
                }
            })
            .collect_view()
    };
    view! {
        <div class="equation">
            {move || join(side(Role::Reactant))}
            <span class="arrow">" -> "</span>
            {move || join(side(Role::Product))}
            {move || {
                let m = side(Role::Modifier);
                (!m.is_empty()).then(|| view! { <span class="modifiers">"  (modified by " {join(m)} ")"</span> })
            }}
        </div>
    }
        .into_any()
}

/// One molecule in a reaction's equation, with its card.
#[component]
fn Chip(reaction: Element, position: usize, species: Element, label: String, class: &'static str) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let key = (reaction, position);
    let pinned = move || ctx.pinned.get() == Some(key);
    let card_id = format!("card-{}-{position}", dom_id(reaction));
    // Where the card opens: below and to the right of the chip unless there
    // is no room there ([`place_card`]).
    let wrap_ref = NodeRef::<leptos::html::Span>::new();
    let card_ref = NodeRef::<leptos::html::Span>::new();
    let up = RwSignal::new(false);
    let end = RwSignal::new(false);
    let place = move || {
        request_animation_frame(move || {
            if let (Some(wrap), Some(card)) = (wrap_ref.get_untracked(), card_ref.get_untracked())
                && let Some((u, e)) = place_card(&wrap, &card)
            {
                up.set(u);
                end.set(e);
            }
        })
    };
    let info = move || s.with(|s| s.as_ref().map(|s| s.molecule_info(species)));
    let accept = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            if let Some(s) = s
                && let Some(t) = s.molecule_info(species).target
            {
                let msg = s.accept(species, t);
                ctx.message.set(Some(msg));
            }
        });
    };
    let reject = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            if let Some(s) = s
                && let Some(t) = s.molecule_info(species).target
            {
                let msg = s.reject(species, t);
                ctx.message.set(Some(msg));
            }
        });
    };
    let show_in_list = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        ctx.pinned.set(None);
        ctx.return_to.set(Some(reaction));
        reveal(ctx, species);
    };
    let card = move || {
        let Some(i) = info() else { return ().into_any() };
        let status = match &i.status {
            Status::Identified { label } => view! { <p>"Identified as " <b>{label.clone()}</b></p> }.into_any(),
            Status::AnnotatedElsewhere { label: Some(label) } => {
                view! { <p>"Identified as " <b>{label.clone()}</b></p> }.into_any()
            }
            Status::AnnotatedElsewhere { label: None } => {
                view! { <p>"Annotated with something outside our database"</p> }.into_any()
            }
            Status::Suggested { label, level } => view! {
                <p>
                    "Suggested: " <b>{label.clone()}</b>
                    <span class=format!("badge {}", i.status.class())>{level_text(*level)}</span>
                </p>
            }
                .into_any(),
            Status::Unknown => view! { <p class="muted">"No suggestion"</p> }.into_any(),
        };
        let suggested = matches!(i.status, Status::Suggested { .. });
        view! {
            <div class="chip-card-body">
                <div class="chip-card-title">
                    <b>{i.name.clone()}</b>
                    {i.id.clone().map(|id| view! { <span class="id">{id}</span> })}
                    {i.compartment.clone().map(|c| view! { <span class="muted">" in " {c}</span> })}
                </div>
                {status}
                {(!i.synonyms.is_empty())
                    .then(|| {
                        let few: Vec<String> = i.synonyms.iter().take(4).cloned().collect();
                        view! { <p class="aka-list">"Also known as: " {few.join("; ")}</p> }
                    })}
                <div class="chip-card-actions">
                    {i.link.clone().map(|url| check_link("Check in database".into(), Some(url)))}
                    {suggested
                        .then(|| {
                            view! {
                                <button class="accept" on:click=accept>"Accept"</button>
                                <button class="reject" title="Not this: do not suggest it again" on:click=reject>
                                    "Reject"
                                </button>
                            }
                        })}
                    <button class="link" on:click=show_in_list>"Show in Molecules list"</button>
                </div>
            </div>
        }
            .into_any()
    };
    view! {
        <span
            class="chip-wrap"
            class:pinned=pinned
            class:up=move || up.get()
            class:end=move || end.get()
            node_ref=wrap_ref
            on:mouseenter=move |_| place()
            on:focusin=move |_| place()
        >
            <button
                class=format!("chip {class}")
                aria-expanded=move || pinned().to_string()
                aria-controls=card_id.clone()
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.stop_propagation();
                    ctx.pinned.update(|p| *p = if *p == Some(key) { None } else { Some(key) });
                    place();
                }
            >
                {label}
            </button>
            <span
                class="chip-card"
                id=card_id.clone()
                node_ref=card_ref
                on:click=|ev: web_sys::MouseEvent| ev.stop_propagation()
            >
                {card}
            </span>
        </span>
    }
}

/// Where a chip's card fits, once it is showing: `(up, end)`, opening
/// upward when there is not room below the chip but more above, and lining
/// up with the chip's right edge when it would run off the right of the
/// window. `None` when the card is not showing (nothing to measure).
fn place_card(wrap: &web_sys::HtmlElement, card: &web_sys::HtmlElement) -> Option<(bool, bool)> {
    let c = card.get_bounding_client_rect();
    if c.height() == 0.0 {
        return None;
    }
    let w = wrap.get_bounding_client_rect();
    let root = document().document_element()?;
    let (view_w, view_h) = (f64::from(root.client_width()), f64::from(root.client_height()));
    let (below, above) = (view_h - w.bottom(), w.top());
    let up = c.height() > below && above > below;
    let end = w.left() + c.width() > view_w && w.right() - c.width() >= 0.0;
    Some((up, end))
}

/// An element's SBML id, and renaming it: every reference in the model
/// follows (reactions, rate laws, rules, events, layouts), as one undoable
/// step, or nothing changes and the reason is shown.
#[component]
fn RenameId(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let current = move || s.with(|s| s.as_ref().map(|s| s.ws.element_id(el).to_string()).unwrap_or_default());
    let start = move |_| {
        draft.set(current());
        error.set(None);
        editing.set(true);
    };
    let cancel = move || {
        editing.set(false);
        error.set(None);
    };
    let commit = move || {
        let new_id = draft.get_untracked();
        let mut result = None;
        s.update(|s| {
            if let Some(s) = s {
                result = Some(s.rename(el, &new_id));
            }
        });
        match result {
            Some(Ok(msg)) => {
                editing.set(false);
                error.set(None);
                ctx.message.set(Some(msg));
            }
            Some(Err(e)) => error.set(Some(e)),
            None => {}
        }
    };
    // One click: rename to the name of what it was identified as.
    let offer = move || s.with(|s| s.as_ref().and_then(|s| s.id_from_identity(el)));
    let rename_to = move |new_id: String| {
        let mut result = None;
        s.update(|s| {
            if let Some(s) = s {
                result = Some(s.rename(el, &new_id));
            }
        });
        match result {
            Some(Ok(msg)) => ctx.message.set(Some(msg)),
            Some(Err(e)) => ctx.message.set(Some(format!("Could not rename to {new_id}: {e}"))),
            None => {}
        }
    };
    let input_id = format!("rename-{}", dom_id(el));
    // Focus the field, with its text selected, when it opens: the user
    // types the new id straight away. (The autofocus attribute only works
    // for elements present when the page loads.)
    let input_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    Effect::new(move |_| {
        if editing.get()
            && let Some(input) = input_ref.get()
        {
            let _ = input.focus();
            input.select();
        }
    });
    view! {
        <div class="rename">
            <Show
                when=move || editing.get()
                fallback=move || {
                    view! {
                        <span class="muted">"SBML id: "</span>
                        <code>{current}</code>
                        <button class="link" on:click=start>"Rename"</button>
                        {move || {
                            offer()
                                .map(|id| {
                                    let label = format!("Rename to {id}");
                                    view! {
                                        " "
                                        <button
                                            class="link"
                                            title="Use the name of what it is identified as as its SBML id"
                                            on:click=move |_| rename_to(id.clone())
                                        >
                                            {label}
                                        </button>
                                    }
                                })
                        }}
                    }
                }
            >
                <label for=input_id.clone()>"New SBML id"</label>
                <input
                    node_ref=input_ref
                    id=input_id.clone()
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    prop:value=move || draft.get()
                    on:input=move |ev| draft.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| match ev.key().as_str() {
                        "Enter" => commit(),
                        "Escape" => cancel(),
                        _ => {}
                    }
                />
                <button class="primary" on:click=move |_| commit()>"Rename"</button>
                <button on:click=move |_| cancel()>"Cancel"</button>
                <p class="hint">
                    "Every reference to it in the model is updated too: reactions, rate laws, rules, events and layout."
                </p>
                {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
            </Show>
        </div>
    }
}

/// Choosing whose proteins UniProt searches cover. Shared by every search
/// (it is part of [`Ctx`]); the model's own organism is offered first when
/// it names one.
#[component]
fn OrganismChoice() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let model_taxon = ctx.session.with_untracked(|s| s.as_ref().and_then(|s| s.ws.model_taxon()));
    let mut options: Vec<(u32, String)> = Vec::new();
    if let Some(t) = model_taxon {
        let name = annot_online::ORGANISMS.iter().find(|(id, _)| *id == t).map(|(_, n)| n.to_string());
        options.push((t, format!("{} (the model's)", name.unwrap_or_else(|| format!("taxon {t}")))));
    }
    for (id, name) in annot_online::ORGANISMS {
        if Some(*id) != model_taxon {
            options.push((*id, name.to_string()));
        }
    }
    view! {
        <label class="organism">
            <span class="muted">"Proteins from "</span>
            <select on:change=move |ev| ctx.organism.set(event_target_value(&ev).parse().ok())>
                <option value="" prop:selected=move || ctx.organism.get().is_none()>"any organism"</option>
                {options
                    .into_iter()
                    .map(|(id, name)| {
                        view! {
                            <option value=id.to_string() prop:selected=move || ctx.organism.get() == Some(id)>
                                {name}
                            </option>
                        }
                    })
                    .collect_view()}
            </select>
        </label>
    }
}

/// How many proteins to show when none has exactly the name searched for.
const PROTEINS_SHOWN: usize = 3;

/// An edit of the session that says what it did, or why it could not.
type SessionEdit = Box<dyn FnOnce(&mut Session) -> Result<String, String>>;

/// The element's annotation, editable: its SBO term, and each MIRIAM term
/// with its qualifier (changeable) and a Remove button, and a row for
/// adding one. Every change is one undo step. The model itself is offered
/// the model qualifiers (`bqmodel:isDescribedBy`, ...) too, and its SBO
/// term is its modelling framework.
#[component]
fn AnnotationEditor(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let terms = move || s.with(|s| s.as_ref().map(|s| s.ws.terms(el)).unwrap_or_default());
    let sbo = move || s.with(|s| s.as_ref().and_then(|s| s.ws.sbo(el)));
    let error = RwSignal::new(None::<String>);
    let add_q = RwSignal::new(qualifier_key(false, "is"));
    let add_text = RwSignal::new(String::new());
    let other_sbo = RwSignal::new(String::new());
    // Runs an edit, showing its message at the bottom or its error here.
    let run = move |f: SessionEdit| {
        let mut out = None;
        s.update(|s| {
            if let Some(s) = s {
                out = Some(f(s));
            }
        });
        match out {
            Some(Ok(msg)) => {
                error.set(None);
                ctx.message.set(Some(msg));
            }
            Some(Err(e)) => error.set(Some(e)),
            None => {}
        }
    };
    let add = move || {
        let (q, text) = (add_q.get_untracked(), add_text.get_untracked());
        run(Box::new(move |s| s.add_term(el, &q, &text)));
        if error.get_untracked().is_none() {
            add_text.set(String::new());
        }
    };
    let set_other_sbo = move || {
        let t = other_sbo.get_untracked();
        match t.trim().trim_start_matches("SBO:").trim_start_matches("sbo:").parse::<u32>() {
            Ok(n) if n > 0 && n < 10_000_000 => {
                run(Box::new(move |s| s.set_sbo(el, Some(n))));
                other_sbo.set(String::new());
            }
            _ => error.set(Some("An SBO term is a number, such as 247 or SBO:0000247.".into())),
        }
    };
    let qualifier_menu = move |current: String, on_pick: Box<dyn Fn(String)>| {
        let mut choices = qualifier_choices(el);
        // A term already there with a qualifier not offered here (a model
        // qualifier on a species) keeps its own.
        if !choices.iter().any(|(k, _)| *k == current) {
            choices.insert(0, (current.clone(), "(as written)"));
        }
        view! {
            <select class="qualifier-menu" on:change=move |ev| on_pick(event_target_value(&ev))>
                {choices
                    .into_iter()
                    .map(|(key, words)| {
                        let selected = key == current;
                        let shown = if words == "(as written)" { key.clone() } else { words.to_string() };
                        view! { <option value=key prop:selected=selected>{shown}</option> }
                    })
                    .collect_view()}
            </select>
        }
    };

    let sbo_row = move || {
        let current = sbo();
        let choices = sbo_choices(el);
        let known = current.is_some_and(|n| choices.iter().any(|(c, _)| *c == n));
        view! {
            <div class="sbo-row">
                <label>
                    <span class="muted">
                        {if el == Element::Model { "Framework (SBO term) " } else { "Kind (SBO term) " }}
                    </span>
                    <select on:change=move |ev| {
                        let v = event_target_value(&ev);
                        let n = v.parse::<u32>().ok();
                        run(Box::new(move |s| s.set_sbo(el, n)));
                    }>
                        <option value="" prop:selected=current.is_none()>"none"</option>
                        {choices
                            .iter()
                            .map(|(n, name)| {
                                view! {
                                    <option value=n.to_string() prop:selected=current == Some(*n)>
                                        {format!("{name} (SBO:{n:07})")}
                                    </option>
                                }
                            })
                            .collect_view()}
                        {current
                            .filter(|_| !known)
                            .map(|n| view! { <option value=n.to_string() prop:selected=true>{format!("SBO:{n:07}")}</option> })}
                    </select>
                </label>
                {current
                    .map(|n| {
                        view! {
                            <a href=format!("https://identifiers.org/SBO:{n:07}") target="_blank" rel="noopener noreferrer">
                                "check"
                            </a>
                        }
                    })}
                <label>
                    <span class="muted">"or another: "</span>
                    <input
                        type="text"
                        class="sbo-number"
                        placeholder="SBO:0000247"
                        prop:value=move || other_sbo.get()
                        on:input=move |ev| other_sbo.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                set_other_sbo();
                            }
                        }
                    />
                </label>
                <button on:click=move |_| set_other_sbo()>"Set"</button>
            </div>
        }
    };

    let term_rows = move || {
        terms()
            .into_iter()
            .map(|t| {
                let uri = t.uri.clone();
                let key = qualifier_key(t.model, &t.qualifier);
                let qualifier = {
                    let (uri, from) = (uri.clone(), key.clone());
                    qualifier_menu(
                        key.clone(),
                        Box::new(move |to: String| {
                            let (uri, from) = (uri.clone(), from.clone());
                            run(Box::new(move |s| s.change_qualifier(el, &uri, &from, &to)));
                        }),
                    )
                };
                let remove = {
                    let (uri, q) = (uri.clone(), key.clone());
                    view! {
                        <button
                            class="link"
                            title="Remove this from the annotation"
                            on:click=move |_| {
                                let (uri, q) = (uri.clone(), q.clone());
                                run(Box::new(move |s| s.remove_term(el, &q, &uri)));
                            }
                        >
                            "Remove"
                        </button>
                    }
                };
                view! {
                    <li>
                        {qualifier}
                        <a href=uri.clone() target="_blank" rel="noopener noreferrer">{uri.clone()}</a>
                        {remove}
                    </li>
                }
            })
            .collect_view()
    };

    view! {
        <div class="annotation">
            <h4>"Annotation"</h4>
            {sbo_row}
            <ul class="terms edit">{term_rows}</ul>
            <div class="add-term">
                <span class="muted">"Add: this "</span>
                {qualifier_menu(add_q.get_untracked(), Box::new(move |q| add_q.set(q)))}
                <input
                    type="text"
                    placeholder="CHEBI:15422, uniprot:P12345 or a URL"
                    prop:value=move || add_text.get()
                    on:input=move |ev| add_text.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        if ev.key() == "Enter" {
                            add();
                        }
                    }
                />
                <button on:click=move |_| add()>"Add"</button>
            </div>
            <p class="hint">
                "\"is\" says it is that entry; \"is a version of\" points to a more general one (glucose for "
                "alpha-D-glucose); \"has part\" lists the members of a complex. Each change can be undone."
            </p>
            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
        </div>
    }
}

/// The model itself: its name and id, its framework and every annotation
/// term ([`AnnotationEditor`]), the papers describing it (with their
/// titles from Europe PMC, on request), the organism it is about, and a
/// search of GO's biological processes for what it describes.
#[component]
fn ModelPanel() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let el = Element::Model;
    let name = move || {
        s.with(|s| {
            s.as_ref().map(|s| {
                let d = s.ws.document();
                let m = d.model.as_ref();
                (
                    m.and_then(|m| m.name.clone()).filter(|n| !n.trim().is_empty()),
                    m.and_then(|m| m.id.clone()).unwrap_or_default(),
                    format!("SBML Level {} Version {}", d.level, d.version),
                )
            })
        })
    };
    let references = move || s.with(|s| s.as_ref().map(|s| s.references()).unwrap_or_default());
    let taxon = move || s.with(|s| s.as_ref().and_then(|s| s.ws.model_taxon()));
    let error = RwSignal::new(None::<String>);
    let ref_text = RwSignal::new(String::new());
    let looking_up = RwSignal::new(false);
    let run = move |f: SessionEdit| {
        let mut out = None;
        s.update(|s| {
            if let Some(s) = s {
                out = Some(f(s));
            }
        });
        match out {
            Some(Ok(msg)) => {
                error.set(None);
                ctx.message.set(Some(msg));
            }
            Some(Err(e)) => error.set(Some(e)),
            None => {}
        }
    };
    let add_reference = move || {
        let text = ref_text.get_untracked();
        run(Box::new(move |s| s.add_reference(&text)));
        if error.get_untracked().is_none() {
            ref_text.set(String::new());
        }
    };
    // Titles of the references, from Europe PMC: only when asked.
    let look_up = move || {
        let wanted: Vec<(String, String)> = s.with_untracked(|s| {
            s.as_ref()
                .map(|s| {
                    s.references()
                        .into_iter()
                        .filter(|u| s.paper(u).is_none())
                        .filter_map(|u| annot_online::paper_url(&u).map(|url| (u, url)))
                        .collect()
                })
                .unwrap_or_default()
        });
        if wanted.is_empty() {
            return;
        }
        looking_up.set(true);
        spawn_local(async move {
            let mut problems = Vec::new();
            for (uri, url) in wanted {
                match browser::fetch_text(&url).await.and_then(|j| annot_online::parse_paper(&j)) {
                    Ok(Some(p)) => s.update(|s| {
                        if let Some(s) = s {
                            s.remember_paper(uri, p);
                        }
                    }),
                    Ok(None) => problems.push(format!("Europe PMC has nothing for {uri}.")),
                    Err(e) => problems.push(format!("Europe PMC: {e}")),
                }
            }
            looking_up.set(false);
            error.set((!problems.is_empty()).then(|| problems.join(" ")));
        });
    };
    let reference_rows = move || {
        references()
            .into_iter()
            .map(|uri| {
                let paper = s.with(|s| s.as_ref().and_then(|s| s.paper(&uri).cloned()));
                let key = s.with_untracked(|s| {
                    s.as_ref()
                        .and_then(|s| {
                            s.ws.terms(el).into_iter().find(|t| t.uri == uri && t.qualifier == "isDescribedBy")
                        })
                        .map(|t| qualifier_key(t.model, &t.qualifier))
                        .unwrap_or_else(|| qualifier_key(true, "isDescribedBy"))
                });
                let remove_uri = uri.clone();
                view! {
                    <li>
                        {match paper {
                            Some(p) => view! {
                                <span class="paper">
                                    <b>{p.title.clone()}</b>
                                    " "
                                    <span class="muted">{p.citation()}</span>
                                </span>
                            }
                                .into_any(),
                            None => ().into_any(),
                        }}
                        <a href=uri.clone() target="_blank" rel="noopener noreferrer">{uri.clone()}</a>
                        <button
                            class="link"
                            on:click=move |_| {
                                let (uri, key) = (remove_uri.clone(), key.clone());
                                run(Box::new(move |s| s.remove_term(el, &key, &uri)));
                            }
                        >
                            "Remove"
                        </button>
                    </li>
                }
            })
            .collect_view()
    };
    let organism_menu = move || {
        let current = taxon();
        let known = current.is_some_and(|t| annot_online::ORGANISMS.iter().any(|(id, _)| *id == t));
        view! {
            <select on:change=move |ev| {
                let t = event_target_value(&ev).parse::<u32>().ok();
                run(Box::new(move |s| s.set_organism(t)));
                ctx.organism.set(t);
            }>
                <option value="" prop:selected=current.is_none()>"not stated"</option>
                {annot_online::ORGANISMS
                    .iter()
                    .map(|(id, n)| {
                        view! { <option value=id.to_string() prop:selected=current == Some(*id)>{*n}</option> }
                    })
                    .collect_view()}
                {current
                    .filter(|_| !known)
                    .map(|t| view! { <option value=t.to_string() prop:selected=true>{format!("taxon {t}")}</option> })}
            </select>
        }
    };
    view! {
        <section class="model-panel">
            {move || {
                name()
                    .map(|(name, id, level)| {
                        view! {
                            <h3>{name.clone().unwrap_or_else(|| id.clone())}</h3>
                            <p class="muted">
                                {name.is_some().then(|| format!("id {id}; "))} {level}
                            </p>
                        }
                    })
            }}
            <AnnotationEditor el=el/>

            <h4>"Papers describing the model"</h4>
            <ul class="terms edit references">{reference_rows}</ul>
            <div class="add-term">
                <input
                    type="text"
                    placeholder="PubMed id, DOI or PMC id"
                    prop:value=move || ref_text.get()
                    on:input=move |ev| ref_text.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        if ev.key() == "Enter" {
                            add_reference();
                        }
                    }
                />
                <button on:click=move |_| add_reference()>"Add paper"</button>
                <button
                    disabled=move || references().is_empty() || looking_up.get()
                    on:click=move |_| look_up()
                >
                    {move || if looking_up.get() { "Looking up..." } else { "Show titles" }}
                </button>
            </div>
            <p class="hint">
                "Saved as bqmodel:isDescribedBy. Show titles asks Europe PMC (www.ebi.ac.uk) about these ids."
            </p>

            <h4>"Organism"</h4>
            <div class="add-term">
                <label><span class="muted">"The model is about "</span> {organism_menu}</label>
            </div>
            <p class="hint">"Saved as bqbiol:hasTaxon; protein searches start with it."</p>
            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}

            <ModelHistory/>
            <ModelNotes/>

            <OnlineSearch el=el/>
        </section>
    }
}

/// Who made the model and when: its creators (name, organization,
/// email) and its created and modified dates, the MIRIAM history. Editing
/// fills in what SBML tools expect (a created date, a modification today).
#[component]
fn ModelHistory() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let history = move || s.with(|s| s.as_ref().map(|s| s.history()).unwrap_or_default());
    let error = RwSignal::new(None::<String>);
    let (given, family, org, email) =
        (RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()));
    let run = move |f: SessionEdit| {
        let mut out = None;
        s.update(|s| {
            if let Some(s) = s {
                out = Some(f(s));
            }
        });
        match out {
            Some(Ok(msg)) => {
                error.set(None);
                ctx.message.set(Some(msg));
            }
            Some(Err(e)) => error.set(Some(e)),
            None => {}
        }
    };
    let add = move || {
        let field = |x: RwSignal<String>| Some(x.get_untracked());
        let c = annot_model::Creator {
            given: field(given),
            family: field(family),
            organization: field(org),
            email: field(email),
        };
        let now = browser::now();
        run(Box::new(move |s| s.add_creator(c, &now)));
        if error.get_untracked().is_none() {
            for x in [given, family, org, email] {
                x.set(String::new());
            }
        }
    };
    let field = move |sig: RwSignal<String>, placeholder: &'static str, kind: &'static str| {
        view! {
            <input
                type=kind
                placeholder=placeholder
                prop:value=move || sig.get()
                on:input=move |ev| sig.set(event_target_value(&ev))
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Enter" {
                        add();
                    }
                }
            />
        }
    };
    let creators = move || {
        history()
            .creators
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let extra: Vec<String> = [c.organization.clone(), c.email.clone()].into_iter().flatten().collect();
                view! {
                    <li>
                        <b>{creator_name(&c)}</b>
                        {(!extra.is_empty()).then(|| view! { <span class="muted">{extra.join(", ")}</span> })}
                        <button
                            class="link"
                            on:click=move |_| {
                                let now = browser::now();
                                run(Box::new(move |s| s.remove_creator(i, &now)));
                            }
                        >
                            "Remove"
                        </button>
                    </li>
                }
            })
            .collect_view()
    };
    let created = move || history().created.map(|c| c.get(..10).unwrap_or(&c).to_string()).unwrap_or_default();
    let modified = move || {
        let m = history().modified;
        if m.is_empty() { "none recorded".to_string() } else { m.iter().map(|d| readable_date(d)).collect::<Vec<_>>().join("; ") }
    };
    view! {
        <h4>"Creators"</h4>
        <ul class="terms edit creators">{creators}</ul>
        <div class="add-term creator-form">
            {field(given, "Given name", "text")}
            {field(family, "Family name", "text")}
            {field(org, "Organization", "text")}
            {field(email, "Email", "email")}
            <button on:click=move |_| add()>"Add creator"</button>
        </div>
        <h4>"Dates"</h4>
        <div class="add-term">
            <label>
                <span class="muted">"Created "</span>
                <input
                    type="date"
                    prop:value=created
                    on:change=move |ev| {
                        let d = event_target_value(&ev);
                        let now = browser::now();
                        run(Box::new(move |s| s.set_created(&d, &now)));
                    }
                />
            </label>
            <span class="muted">"Modified: " {modified}</span>
            <button on:click=move |_| {
                let now = browser::now();
                run(Box::new(move |s| s.record_modified(&now)));
            }>"Record today as modified"</button>
        </div>
        <p class="hint">
            "Saved as the model's history (dcterms and vCard in its annotation). SBML tools expect a creator, a "
            "created date and a modified date, so changing these fills in a missing created date and today's "
            "modification. Each change can be undone."
        </p>
        {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
    }
}

/// The model's notes, as plain text: paragraphs separated by blank lines.
/// Saved as XHTML paragraphs; notes with links, lists or tables are shown
/// as text, and a warning says saving replaces that formatting.
#[component]
fn ModelNotes() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let saved = Memo::new(move |_| s.with(|s| s.as_ref().map(|s| s.notes()).unwrap_or_default()));
    let draft = RwSignal::new(saved.get_untracked().0);
    // Undo, redo or saving changes the notes: show them.
    Effect::new(move |_| draft.set(saved.get().0));
    let changed = move || draft.get() != saved.get().0;
    let save = move |_| {
        let text = draft.get_untracked();
        let mut msg = None;
        s.update(|s| {
            if let Some(s) = s {
                msg = Some(s.set_notes(&text));
            }
        });
        match msg {
            Some(Ok(m)) | Some(Err(m)) => ctx.message.set(Some(m)),
            None => {}
        }
    };
    view! {
        <h4>"Notes"</h4>
        {move || {
            saved
                .get()
                .1
                .then(|| {
                    view! {
                        <p class="hint warn">
                            "These notes have formatting (links, lists or tables), shown here as text. Saving "
                            "replaces them with plain paragraphs; Undo brings the original back."
                        </p>
                    }
                })
        }}
        <textarea
            class="notes"
            rows="6"
            placeholder="What the model is, where it comes from, how it was fitted..."
            prop:value=move || draft.get()
            on:input=move |ev| draft.set(event_target_value(&ev))
        ></textarea>
        <div class="add-term">
            <button class="primary" disabled=move || !changed() on:click=save>"Save notes"</button>
            <button disabled=move || !changed() on:click=move |_| draft.set(saved.get_untracked().0)>"Revert"</button>
            <span class="muted">"A blank line starts a new paragraph."</span>
        </div>
    }
}

/// What a search of other databases has come to.
#[derive(Clone)]
enum Found {
    Searching,
    /// The entries, best first, and anything that went wrong (one service
    /// failing does not hide the other's answer).
    Done {
        hits: Vec<annot_online::Hit>,
        problems: Vec<String>,
        /// A note on how the search went ("no reaction has all of them").
        note: Option<String>,
    },
}

/// Searching databases beyond the knowledge base, for a molecule or
/// reaction the annotator knows nothing (or not enough) about: ChEBI and
/// UniProt for a molecule, Rhea for a reaction. Nothing is sent anywhere
/// until the user presses Search; what is sent, and to whom, is said
/// beside the button.
#[component]
fn OnlineSearch(el: Element) -> impl IntoView {
    use annot_online::{Db, RheaQuery};
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let reaction = matches!(el, Element::Reaction(_));
    let compartment = matches!(el, Element::Compartment(_));
    let whole_model = el == Element::Model;
    let text = RwSignal::new(s.with_untracked(|s| s.as_ref().map(|s| s.search_text(el)).unwrap_or_default()));
    let enzyme = RwSignal::new(s.with_untracked(|s| s.as_ref().map(|s| s.enzyme_text(el)).unwrap_or_default()));
    let found = RwSignal::new(None::<Found>);
    // For a reaction: its molecules, and which are identified (the search
    // uses their ChEBI ids, else their names).
    let parts = move || s.with(|s| s.as_ref().map(|s| s.rhea_participants(el)).unwrap_or_default());

    let search_molecule = move || {
        let q = text.get_untracked().trim().to_string();
        if q.is_empty() {
            return;
        }
        let taxon = ctx.organism.get_untracked();
        found.set(Some(Found::Searching));
        spawn_local(async move {
            let mut hits = Vec::new();
            let mut problems = Vec::new();
            if compartment || whole_model {
                let url = if whole_model { annot_online::go_process_url(&q) } else { annot_online::go_url(&q) };
                match browser::fetch_text(&url).await.and_then(|j| annot_online::parse_go(&j, &q)) {
                    Ok(mut h) => {
                        annot_online::exact_first(&mut h);
                        hits.extend(h);
                    }
                    Err(e) => problems.push(format!("GO: {e}")),
                }
                found.set(Some(Found::Done { hits, problems, note: None }));
                return;
            }
            match browser::fetch_text(&annot_online::chebi_url(&q)).await.and_then(|j| annot_online::parse_chebi(&j, &q)) {
                Ok(mut h) => {
                    annot_online::exact_first(&mut h);
                    hits.extend(h);
                }
                Err(e) => problems.push(format!("ChEBI: {e}")),
            }
            let mut note = None;
            match browser::fetch_text(&annot_online::uniprot_url(&q, taxon))
                .await
                .and_then(|t| annot_online::parse_uniprot(&t, &q))
            {
                Ok(mut h) => {
                    annot_online::exact_first(&mut h);
                    // Proteins that only mention the words (a molecule's
                    // name in an enzyme's) are noise: a few at most.
                    if !h.iter().any(|h| h.exact) && h.len() > PROTEINS_SHOWN {
                        h.truncate(PROTEINS_SHOWN);
                        note = Some(format!(
                            "No protein has exactly this name; showing the first {PROTEINS_SHOWN} UniProt finds."
                        ));
                    }
                    hits.extend(h);
                }
                Err(e) => problems.push(format!("UniProt: {e}")),
            }
            found.set(Some(Found::Done { hits, problems, note }));
        });
    };
    let search_reaction = move || {
        let parts = parts();
        let Some(query) = RheaQuery::for_reaction(&parts) else {
            found.set(Some(Found::Done {
                hits: Vec::new(),
                problems: vec!["This reaction has no molecules to search by.".into()],
                note: None,
            }));
            return;
        };
        found.set(Some(Found::Searching));
        spawn_local(async move {
            let ranked = |q: annot_online::RheaQuery| {
                let parts = parts.clone();
                async move {
                    let tsv = browser::fetch_text(&annot_online::rhea_url(&q)).await?;
                    s.with_untracked(|s| match s {
                        Some(s) => annot_online::rank_rhea(&tsv, &parts, |id| s.chebi_is_currency(id)),
                        None => Ok(Vec::new()),
                    })
                }
            };
            let mut note = None;
            let mut result = ranked(query.clone()).await;
            // Nothing has all of them: one may be identified with an entry
            // Rhea does not use. Try leaving each out in turn.
            if matches!(&result, Ok(h) if h.is_empty()) && !query.relaxed().is_empty() {
                let mut lists = Vec::new();
                for q in query.relaxed() {
                    match ranked(q).await {
                        Ok(h) => lists.push(h),
                        Err(e) => {
                            result = Err(e);
                            break;
                        }
                    }
                }
                if result.is_ok() {
                    note = Some("No Rhea reaction has all of these molecules; these have all but one.".to_string());
                    result = Ok(annot_online::merge_ranked(lists));
                }
            }
            found.set(Some(match result {
                Ok(hits) => Found::Done { hits, problems: Vec::new(), note },
                Err(e) => Found::Done { hits: Vec::new(), problems: vec![format!("Rhea: {e}")], note: None },
            }));
        });
    };
    let search = move || if reaction { search_reaction() } else { search_molecule() };
    // A reaction by its enzyme: an EC number goes straight to Rhea; a name
    // is looked up in UniProt for its EC numbers first.
    let search_enzyme = move || {
        let q = enzyme.get_untracked().trim().to_string();
        if q.is_empty() {
            return;
        }
        let parts = parts();
        let taxon = ctx.organism.get_untracked();
        found.set(Some(Found::Searching));
        spawn_local(async move {
            let done = |hits, problems: Vec<String>, note: Option<String>| {
                found.set(Some(Found::Done { hits, problems, note }));
            };
            let (ecs, from) = match annot_online::ec_from_text(&q) {
                Some(ec) => (vec![ec], None),
                None => {
                    let proteins = browser::fetch_text(&annot_online::uniprot_ec_url(&q, taxon))
                        .await
                        .and_then(|t| annot_online::parse_uniprot(&t, &q));
                    match proteins {
                        Ok(p) => (annot_online::ecs_of_proteins(&p), Some(q.clone())),
                        Err(e) => return done(Vec::new(), vec![format!("UniProt: {e}")], None),
                    }
                }
            };
            if ecs.is_empty() {
                let note = format!(
                    "No reviewed protein named \"{q}\" has an EC number{}. Try another name, or an EC number.",
                    if taxon.is_some() { " in this organism" } else { "" }
                );
                return done(Vec::new(), Vec::new(), Some(note));
            }
            let query = RheaQuery::ByEc(ecs.clone());
            let result = browser::fetch_text(&annot_online::rhea_url(&query)).await.and_then(|tsv| {
                s.with_untracked(|s| match s {
                    Some(s) => annot_online::rank_rhea(&tsv, &parts, |id| s.chebi_is_currency(id)),
                    None => Ok(Vec::new()),
                })
            });
            let ecs = ecs.join(", ");
            let note = match from {
                Some(name) => format!("Rhea reactions of EC {ecs}, the activity of proteins named \"{name}\" in UniProt."),
                None => format!("Rhea reactions of EC {ecs}."),
            };
            match result {
                Ok(hits) => done(hits, Vec::new(), Some(note)),
                Err(e) => done(Vec::new(), vec![format!("Rhea: {e}")], None),
            }
        });
    };
    let organism_choice = move || view! { <OrganismChoice/> };

    let form = move || {
        if reaction {
            let parts = parts();
            let main: Vec<_> = parts.iter().filter(|p| !p.currency).cloned().collect();
            let main = if main.is_empty() { parts } else { main };
            let by_ids = main.iter().any(|p| !p.chebi.is_empty());
            let listed = main
                .iter()
                .map(|p| {
                    let known = !p.chebi.is_empty();
                    view! {
                        <span class="chip" class:done=known class:unknown=!known>
                            {p.name.clone()}
                        </span>
                    }
                })
                .collect_view();
            let how = if by_ids {
                "Looks for Rhea reactions with every identified molecule (green), and ranks them by how well both sides fit."
            } else {
                "None of these molecules is identified yet, so Rhea is searched by their names. Identifying them first gives much better results."
            };
            view! {
                <div class="online-form">
                    <span class="muted">"Molecules: "</span>
                    {listed}
                    <button class="primary" on:click=move |_| search()>"Search Rhea"</button>
                </div>
                <p class="hint">{how} " Sends their ChEBI ids or names to " {Db::Rhea.host()} "."</p>
                <div class="online-form">
                    <label>
                        <span class="muted">"Or by enzyme name or EC number "</span>
                        <input
                            type="text"
                            placeholder="phosphofructokinase, or 2.7.1.11"
                            prop:value=move || enzyme.get()
                            on:input=move |ev| enzyme.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    search_enzyme();
                                }
                            }
                        />
                    </label>
                    {organism_choice}
                    <button on:click=move |_| search_enzyme()>"Search by enzyme"</button>
                </div>
                <p class="hint">
                    "A name is looked up in UniProt for the enzyme's EC numbers, then Rhea lists the reactions of those numbers. "
                    "Sends the text to " {Db::Uniprot.host()} " and " {Db::Rhea.host()} "."
                </p>
            }
                .into_any()
        } else if whole_model {
            view! {
                <div class="online-form">
                    <label>
                        <span class="muted">"Search GO's processes for "</span>
                        <input
                            type="text"
                            prop:value=move || text.get()
                            on:input=move |ev| text.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    search();
                                }
                            }
                        />
                    </label>
                    <button class="primary" on:click=move |_| search()>"Search GO"</button>
                </div>
                <p class="hint">
                    "The Gene Ontology's biological processes (\"glycolytic process\"): what the model as a whole "
                    "describes. Accepting adds it as \"is a version of\". Sends the text to " {Db::Go.host()} "."
                </p>
            }
                .into_any()
        } else if compartment {
            view! {
                <div class="online-form">
                    <label>
                        <span class="muted">"Search GO for "</span>
                        <input
                            type="text"
                            prop:value=move || text.get()
                            on:input=move |ev| text.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    search();
                                }
                            }
                        />
                    </label>
                    <button class="primary" on:click=move |_| search()>"Search GO"</button>
                </div>
                <p class="hint">
                    "The Gene Ontology's cellular components: compartments and structures of the cell. "
                    "Sends the text to " {Db::Go.host()} "."
                </p>
            }
                .into_any()
        } else {
            view! {
                <div class="online-form">
                    <label>
                        <span class="muted">"Search for "</span>
                        <input
                            type="text"
                            prop:value=move || text.get()
                            on:input=move |ev| text.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    search();
                                }
                            }
                        />
                    </label>
                    {organism_choice}
                    <button class="primary" on:click=move |_| search()>"Search ChEBI and UniProt"</button>
                </div>
                <p class="hint">
                    "ChEBI has small molecules beyond the knowledge base; UniProt has proteins (enzymes, transporters). "
                    "Sends the text to " {Db::Chebi.host()} " and " {Db::Uniprot.host()} "."
                </p>
            }
                .into_any()
        }
    };

    let results = move || {
        let Some(f) = found.get() else { return ().into_any() };
        let (hits, problems, note) = match f {
            Found::Searching => return view! { <p class="muted" role="status">"Searching..."</p> }.into_any(),
            Found::Done { hits, problems, note } => (hits, problems, note),
        };
        let current = s.with(|s| s.as_ref().map(|s| s.ws.identity_uris(el)).unwrap_or_default());
        let nothing = hits.is_empty() && problems.is_empty();
        let mut last_db = None;
        let rows = hits
            .into_iter()
            .map(|h| {
                let heading = (last_db != Some(h.db)).then(|| {
                    let what = match h.db {
                        Db::Chebi => "ChEBI: molecules",
                        Db::Uniprot => "UniProt: proteins",
                        Db::Rhea => "Rhea: reactions",
                        Db::Go if whole_model => "GO: biological processes",
                        Db::Go => "GO: cellular components",
                    };
                    view! { <h5>{what}</h5> }
                });
                last_db = Some(h.db);
                let accepted = current.contains(&h.uri);
                let id = h.id.clone();
                let uri = h.uri.clone();
                let detail = h.detail.clone();
                let why = h.why.clone();
                let aka: Vec<String> = h.synonyms.iter().take(4).cloned().collect();
                let label = h.label.clone();
                let exact = h.exact;
                let action = if accepted {
                    view! { <span class="accepted" role="status">" Accepted"</span> }.into_any()
                } else {
                    view! {
                        <button
                            class="accept"
                            title="Annotate the model with this entry"
                            on:click=move |_| {
                                s.update(|s| {
                                    if let Some(s) = s {
                                        let msg = s.accept_online(el, &h);
                                        if let Some((name, label)) = s.online_entry(&h.uri) {
                                            browser::remember_name(&h.uri, &name, &label);
                                        }
                                        ctx.message.set(Some(msg));
                                    }
                                })
                            }
                        >
                            "Accept"
                        </button>
                    }
                        .into_any()
                };
                view! {
                    {heading}
                    <div class="cand online" class:is-accepted=accepted>
                        <div class="cand-head">
                            {check_link(label, Some(uri.clone()))}
                            <span class="ids">
                                <a href=uri target="_blank" rel="noopener noreferrer">{id}</a>
                            </span>
                            {exact.then(|| view! { <span class="badge high">{if reaction { "fits" } else { "same name" }}</span> })}
                            {action}
                        </div>
                        {detail.map(|d| view! { <p class="muted detail">{d}</p> })}
                        {(!aka.is_empty()).then(|| view! { <p class="aka-list">"Also known as: " {aka.join("; ")}</p> })}
                        {why.map(|w| view! { <ul class="evidence"><li>{w}</li></ul> })}
                    </div>
                }
            })
            .collect_view();
        view! {
            {problems.into_iter().map(|p| view! { <p class="error" role="alert">{p}</p> }).collect_view()}
            {nothing.then(|| view! { <p class="muted">"Nothing found. Try another name or spelling."</p> })}
            {note.map(|n| view! { <p class="muted">{n}</p> })}
            {rows}
        }
            .into_any()
    };

    view! {
        <div class="online">
            <h4>"Search other databases"</h4>
            {form}
            {results}
        </div>
    }
}

/// The candidates for an element, with their evidence, and its existing
/// annotation.
#[component]
fn Details(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let rows = move || {
        s.with(|s| {
            let Some(s) = s else { return Vec::new() };
            let current = s.identified_as(el);
            s.candidates(el)
                .iter()
                .take(5)
                .map(|c| {
                    (
                        c.target,
                        s.label(c.target),
                        s.links(c.target),
                        s.synonyms(c.target),
                        c.evidence.iter().map(|e| e.note.clone()).collect::<Vec<_>>(),
                        // Is it the current identification, and is there one?
                        (current.contains(&c.target), !current.is_empty()),
                    )
                })
                .collect::<Vec<_>>()
        })
    };
    view! {
        <div class="details">
            <RenameId el=el/>
            <AnnotationEditor el=el/>
            <Show when=move || !matches!(el, Element::Compartment(_))>
            <h4>"Suggestions"</h4>
            {move || {
                let rows = rows();
                if rows.is_empty() {
                    return view! { <p class="muted">"The annotator has nothing to suggest for this one."</p> }.into_any();
                }
                rows.into_iter()
                    .enumerate()
                    .map(|(i, (target, label, links, aka, notes, (accepted, identified)))| {
                        let main = links.first().map(|(_, url)| url.clone());
                        let action = if accepted {
                            view! {
                                <span class="accepted" role="status">
                                    <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                                        <path d="M2 6.5l2.5 2.5L10 3.5" fill="none" stroke="currentColor" stroke-width="1.8"/>
                                    </svg>
                                    " Accepted"
                                </span>
                            }
                                .into_any()
                        } else {
                            view! {
                                <span class="cand-actions">
                                    <button
                                        class="accept"
                                        title=if identified {
                                            "Replace the current identification with this one"
                                        } else {
                                            "Annotate the model with this"
                                        }
                                        on:click=move |_| {
                                            s.update(|s| {
                                                if let Some(s) = s {
                                                    let msg = s.accept(el, target);
                                                    ctx.message.set(Some(msg));
                                                }
                                            })
                                        }
                                    >
                                        {if identified { "Use this instead" } else { "Accept" }}
                                    </button>
                                    <button
                                        class="reject"
                                        title="Not this: do not suggest it again"
                                        on:click=move |_| {
                                            s.update(|s| {
                                                if let Some(s) = s {
                                                    let msg = s.reject(el, target);
                                                    ctx.message.set(Some(msg));
                                                }
                                            })
                                        }
                                    >
                                        "Reject"
                                    </button>
                                </span>
                            }
                                .into_any()
                        };
                        view! {
                            <div class="cand" class:is-accepted=accepted>
                                <div class="cand-head">
                                    <span class="rank">{i + 1}</span>
                                    {check_link(label, main)}
                                    <span class="ids">
                                        {links
                                            .into_iter()
                                            .map(|(text, url)| {
                                                view! {
                                                    <a href=url target="_blank" rel="noopener noreferrer">
                                                        {text}
                                                    </a>
                                                }
                                            })
                                            .collect_view()}
                                    </span>
                                    {action}
                                </div>
                                {(!aka.is_empty())
                                    .then(|| view! { <p class="aka-list">"Also known as: " {aka.join("; ")}</p> })}
                                <ul class="evidence">
                                    {notes.into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}
                                </ul>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
            </Show>
            <OnlineSearch el=el/>
            {move || {
                let rejected = s.with(|s| s.as_ref().map(|s| s.rejected(el)).unwrap_or_default());
                (!rejected.is_empty())
                    .then(|| {
                        view! {
                            <h4>"Rejected"</h4>
                            <ul class="rejected">
                                {rejected
                                    .into_iter()
                                    .map(|(target, label)| {
                                        view! {
                                            <li>
                                                <span>{label}</span>
                                                <button
                                                    class="link"
                                                    on:click=move |_| {
                                                        s.update(|s| {
                                                            if let Some(s) = s {
                                                                let msg = s.restore(el, target);
                                                                ctx.message.set(Some(msg));
                                                            }
                                                        })
                                                    }
                                                >
                                                    "Restore"
                                                </button>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                    })
            }}
        </div>
    }
}

/// Whether a key press is typing (in a text box, menu or editable text),
/// which shortcuts leave alone.
fn typing(ev: &web_sys::KeyboardEvent) -> bool {
    use wasm_bindgen::JsCast;
    let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok()) else {
        return false;
    };
    matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") || el.is_content_editable()
}

/// Whether the key press is on a button or link, where Enter belongs to it.
fn on_control(ev: &web_sys::KeyboardEvent) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|el| matches!(el.tag_name().as_str(), "BUTTON" | "A" | "SUMMARY"))
}

/// Scrolls a row into view, as little as needed.
fn scroll_to(el: Element) {
    set_timeout(
        move || {
            if let Some(e) = document().get_element_by_id(&dom_id(el)) {
                let opts = web_sys::ScrollIntoViewOptions::new();
                opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
                e.scroll_into_view_with_scroll_into_view_options(&opts);
            }
        },
        Duration::from_millis(30),
    );
}

/// The keyboard shortcuts (listed in [`Shortcuts`]). Keys typed into a
/// text box or menu are left alone, and so is Enter on a button.
fn on_key(ctx: Ctx, ev: web_sys::KeyboardEvent) {
    if ev.alt_key() || ev.meta_key() || typing(&ev) {
        return;
    }
    let s = ctx.session;
    let key = ev.key();
    if ev.ctrl_key() {
        let redo = matches!(key.as_str(), "y" | "Y") || (ev.shift_key() && matches!(key.as_str(), "z" | "Z"));
        let undo = !ev.shift_key() && matches!(key.as_str(), "z" | "Z");
        if !(undo || redo) || s.with_untracked(|s| s.is_none()) {
            return;
        }
        ev.prevent_default();
        s.update(|s| {
            if let Some(s) = s {
                if redo { s.redo() } else { s.undo() }
            }
        });
        return;
    }
    match key.as_str() {
        "?" => ctx.help.update(|h| *h = !*h),
        "Escape" => {
            if ctx.guide.get_untracked() {
                ctx.guide.set(false);
            } else if ctx.about.get_untracked() {
                ctx.about.set(false);
            } else if ctx.help.get_untracked() {
                ctx.help.set(false);
            } else if ctx.pinned.get_untracked().is_some() {
                ctx.pinned.set(None);
            } else if ctx.selected.get_untracked().is_some() {
                ctx.selected.set(None);
            } else {
                return;
            }
        }
        _ if s.with_untracked(|s| s.is_none()) => return,
        "j" | "ArrowDown" => move_cursor(ctx, 1),
        "k" | "ArrowUp" => move_cursor(ctx, -1),
        "n" => next_to_review(ctx),
        "Enter" | "o" => {
            if key == "Enter" && on_control(&ev) {
                return;
            }
            let Some(el) = ctx.cursor.get_untracked() else { return };
            ctx.selected.update(|sel| *sel = if *sel == Some(el) { None } else { Some(el) });
            scroll_to(el);
        }
        "a" => act_on_cursor(ctx, true),
        "r" => act_on_cursor(ctx, false),
        "/" => {
            let Some(el) = ctx.cursor.get_untracked().or(ctx.selected.get_untracked()) else { return };
            ctx.selected.set(Some(el));
            set_timeout(
                move || {
                    let sel = format!("#{} .online input", dom_id(el));
                    if let Ok(Some(input)) = document().query_selector(&sel) {
                        use wasm_bindgen::JsCast;
                        // Selected, so typing replaces it and Enter searches.
                        if let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() {
                            let _ = input.focus();
                            input.select();
                            input.scroll_into_view();
                        }
                    }
                },
                Duration::from_millis(50),
            );
        }
        "1" | "2" | "3" | "4" => {
            ctx.tab.set(match key.as_str() {
                "1" => Tab::Molecules,
                "2" => Tab::Reactions,
                "3" => Tab::Compartments,
                _ => Tab::Model,
            });
            ctx.cursor.set(None);
        }
        _ => return,
    }
    ev.prevent_default();
}

/// The rows showing now.
fn visible_rows(ctx: Ctx) -> Vec<Element> {
    let (tab, filter) = (ctx.tab.get_untracked(), ctx.filter.get_untracked());
    ctx.session.with_untracked(|s| s.as_ref().map(|s| rows_of(s, tab, filter)).unwrap_or_default())
}

/// Moves the keyboard cursor `by` rows (from the first row if none).
fn move_cursor(ctx: Ctx, by: isize) {
    let rows = visible_rows(ctx);
    if rows.is_empty() {
        return;
    }
    let at = ctx.cursor.get_untracked().and_then(|c| rows.iter().position(|r| *r == c));
    let next = match at {
        Some(i) => (i as isize + by).clamp(0, rows.len() as isize - 1) as usize,
        None => 0,
    };
    ctx.cursor.set(Some(rows[next]));
    scroll_to(rows[next]);
}

/// Moves the cursor to the next row with a suggestion to review, wrapping
/// round.
fn next_to_review(ctx: Ctx) {
    let rows = visible_rows(ctx);
    let start = ctx.cursor.get_untracked().and_then(|c| rows.iter().position(|r| *r == c)).map_or(0, |i| i + 1);
    let found = ctx.session.with_untracked(|s| {
        let s = s.as_ref()?;
        (0..rows.len()).map(|k| rows[(start + k) % rows.len()]).find(|el| matches!(s.status(*el), Status::Suggested { .. }))
    });
    match found {
        Some(el) => {
            ctx.cursor.set(Some(el));
            scroll_to(el);
        }
        None => ctx.message.set(Some("Nothing left to review in this list.".into())),
    }
}

/// Accepts (or rejects) the suggestion on the cursor's row, then moves to
/// the next row.
fn act_on_cursor(ctx: Ctx, accept: bool) {
    let Some(el) = ctx.cursor.get_untracked() else {
        ctx.message.set(Some("Pick a row first: press j or k, or click one.".into()));
        return;
    };
    let rows = visible_rows(ctx);
    let next = rows.iter().position(|r| *r == el).and_then(|i| rows.get(i + 1)).copied();
    ctx.session.update(|s| {
        let Some(s) = s else { return };
        if !matches!(s.status(el), Status::Suggested { .. }) {
            let what = if accept { "accept" } else { "reject" };
            ctx.message.set(Some(format!("There is no suggestion to {what} for {}.", s.display_name(el))));
            return;
        }
        let msg = match (el, accept) {
            (Element::Compartment(_), true) => s.accept_compartment(el),
            (Element::Compartment(_), false) => s.reject_compartment(el),
            _ => match s.candidates(el).first().map(|c| c.target) {
                Some(t) if accept => s.accept(el, t),
                Some(t) => s.reject(el, t),
                None => return,
            },
        };
        ctx.message.set(Some(msg));
    });
    if let Some(n) = next {
        ctx.cursor.set(Some(n));
        scroll_to(n);
    }
}

/// What the app is, where its data comes from, and the licenses: every
/// source in the knowledge pack's header with the notice its license
/// requires (BiGG's legacy license asks for its full notice in all
/// copies), and the services searched on request.
#[component]
fn About() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let sources = move || {
        ctx.pack_header.with(|h| {
            h.as_ref()
                .map(|h| {
                    h.sources
                        .iter()
                        .map(|src| {
                            let notice = src.notice.clone();
                            view! {
                                <section class="source">
                                    <h4>
                                        <a href=src.url.clone() target="_blank" rel="noopener noreferrer">{src.name.clone()}</a>
                                        <span class="muted">{format!(" (version {})", src.version)}</span>
                                    </h4>
                                    <p>"License: " {src.license.clone()}</p>
                                    {notice.map(|n| view! { <pre class="notice">{n}</pre> })}
                                </section>
                            }
                        })
                        .collect_view()
                })
        })
    };
    let built = move || ctx.pack_header.with(|h| h.as_ref().map(|h| h.built.clone()));
    view! {
        <Show when=move || ctx.about.get()>
            <div class="about-backdrop" on:click=move |_| ctx.about.set(false)></div>
            <div class="about" role="dialog" aria-label="About PathwayAnnotator">
                <div class="shortcuts-head">
                    <h3>"About PathwayAnnotator"</h3>
                    <button class="link" on:click=move |_| ctx.about.set(false)>"Close"</button>
                </div>
                <p class="version">"Version " {env!("CARGO_PKG_VERSION")}</p>
                <p>
                    "Suggests what each molecule, reaction and compartment of an SBML or Antimony model is, "
                    "with the evidence, for you to confirm. It runs in your browser: your model is not uploaded. "
                    <a href="https://github.com/sys-bio/PathwayAnnotator" target="_blank" rel="noopener noreferrer">
                        "Source code"
                    </a>
                    " (MIT or Apache-2.0)."
                </p>
                <p>"Conceived by Herbert Sauro, assisted by Claude Code."</p>
                <p>
                    "To cite: Sauro, H. M. (2026). PathwayAnnotator: A Web-based SBML Annotation Tool for "
                    "Biochemical Networks. Zenodo. "
                    <a href="https://doi.org/10.5281/zenodo.24237183" target="_blank" rel="noopener noreferrer">
                        "doi:10.5281/zenodo.24237183"
                    </a>
                </p>
                <p>
                    "This work was supported by NIH Biomedical Imaging and Bioengineering award P41 EB023912 "
                    "through HMS at the Center for Reproducible Biomedical Modeling "
                    <a href="https://reproduciblebiomodels.org" target="_blank" rel="noopener noreferrer">
                        "reproduciblebiomodels.org"
                    </a>
                </p>
                <h3>"Data in the knowledge base"</h3>
                <p class="muted">{move || built().map(|b| format!("Built {b} from:"))}</p>
                {sources}
                <h3>"Searched only when you ask"</h3>
                <p>
                    "Search other databases (in Details) sends the search text, or ChEBI ids and EC numbers, to: "
                    "EBI's Ontology Lookup Service for ChEBI and the Gene Ontology, UniProt, and Rhea "
                    "(all CC BY 4.0). Nothing is sent otherwise."
                </p>
            </div>
        </Show>
    }
}

/// A few lines at the top of the annotation page saying what the tool is
/// for and how to start, so it does not look mysterious; the Help page has
/// the rest.
#[component]
fn Intro() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    view! {
        <section class="intro">
            <p>
                "Annotating a model links each molecule, reaction and compartment to the database entry it "
                "stands for (ChEBI, Rhea, KEGG, GO and others), so people and software can tell what it is. "
                "Annotated models are easier to find, check, reuse and combine; the curated models in BioModels "
                "are annotated this way."
            </p>
            <p>
                "Below, each element has a suggestion with a confidence level. Open "
                <b>"Details"</b>
                " to see the evidence and check the entry, then "
                <b>"Accept"</b>
                " or "
                <b>"Reject"</b>
                ". A good start: accept all high-confidence suggestions, then work through the rest. When you "
                "are done, save the annotated SBML. "
                <button class="link" on:click=move |_| ctx.guide.set(true)>"More help"</button>
            </p>
        </section>
    }
}

/// How to use the annotator, step by step: what annotation is for, a way
/// to work through a model, what the confidence levels mean, what to do
/// when there is no suggestion, editing, the model itself, and privacy.
#[component]
fn Help() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    view! {
        <Show when=move || ctx.guide.get()>
            <div class="about-backdrop" on:click=move |_| ctx.guide.set(false)></div>
            <div class="about help" role="dialog" aria-label="Help">
                <div class="shortcuts-head">
                    <h3>"How to use PathwayAnnotator"</h3>
                    <button class="link" on:click=move |_| ctx.guide.set(false)>"Close"</button>
                </div>

                <h4>"What it is for"</h4>
                <p>
                    "A model's species and reactions usually have short names (G6P, vPFK) that only make sense to "
                    "the person who wrote them. Annotations link each one to a database entry: G6P is ChEBI "
                    "CHEBI:4170, D-glucose 6-phosphate. They are stored in the SBML file in a standard form (MIRIAM "
                    "annotations and SBO terms), so other people and tools can tell exactly what the model contains, "
                    "compare it with other models, and reuse its parts."
                </p>

                <h4>"A good way to work"</h4>
                <ol>
                    <li>"Open your model (SBML or Antimony). It stays on your computer."</li>
                    <li>
                        "Press " <b>"Accept all high-confidence suggestions"</b>
                        ". In tests these were right about 99% of the time. Undo reverts them in one step."
                    </li>
                    <li>
                        "Work through the rest of " <b>"Molecules"</b> ": open " <b>"Details"</b>
                        " to see why each suggestion was made, follow the link to check the database entry, then "
                        "Accept, pick another candidate, or Reject. Synonyms are shown, so D-glucopyranose is "
                        "recognizable as glucose."
                    </li>
                    <li>
                        "Then " <b>"Reactions"</b>
                        ". Confirmed molecules make reaction suggestions much better: a reaction whose molecules are "
                        "all confirmed and that matches one database reaction becomes high confidence. Hover over a "
                        "molecule in a reaction to check it without leaving the reaction."
                    </li>
                    <li>"Then " <b>"Compartments"</b> " (GO cellular components)."</li>
                    <li>
                        "Then " <b>"Model"</b>
                        ": the modelling framework, the papers describing the model, its organism, the process it "
                        "describes, its creators and dates, and notes."
                    </li>
                    <li>"Press " <b>"Save annotated SBML"</b> " to download the annotated model."</li>
                </ol>

                <h4>"Confidence levels"</h4>
                <ul>
                    <li><b>"High confidence"</b>": strong, unambiguous evidence; right about 99% of the time in tests."</li>
                    <li><b>"Likely"</b>": good evidence; check it."</li>
                    <li><b>"Uncertain"</b>": weak or conflicting evidence; check it carefully."</li>
                    <li><b>"No suggestion"</b>": the knowledge base has nothing; search other databases (below)."</li>
                </ul>
                <p>
                    "The annotator learns from you: each molecule or reaction you confirm or reject improves the "
                    "suggestions for the elements around it."
                </p>

                <h4>"When there is no suggestion"</h4>
                <p>
                    "Details has " <b>"Search other databases"</b>
                    ": all of ChEBI and reviewed UniProt proteins for a molecule, Rhea for a reaction (by its "
                    "molecules, or by enzyme name or EC number), and GO for a compartment or the model's process. "
                    "Nothing is sent anywhere until you press Search; the text beside the button says what is sent "
                    "and to whom."
                </p>

                <h4>"Editing"</h4>
                <ul>
                    <li>
                        <b>"Annotation"</b>
                        " (in Details) shows every term. Change a qualifier: \"is\" means it is that entry, "
                        "\"is a version of\" points to a more general one (glucose for alpha-D-glucose), \"has part\" "
                        "lists the members of a complex. Remove a term, or add one by typing an id such as "
                        "CHEBI:15422 or uniprot:P12345."
                    </li>
                    <li>
                        <b>"Kind (SBO term)"</b>
                        " says what sort of thing an element is: simple chemical, protein, biochemical reaction, ..."
                    </li>
                    <li>
                        <b>"Rename"</b>
                        " (in Details) changes an element's SBML id everywhere it is used, rate laws included."
                    </li>
                    <li>"Every change can be undone (Undo, or Ctrl+Z)."</li>
                    <li>
                        "Keyboard: j and k move between rows, a accepts, r rejects; press ? for the full list."
                    </li>
                </ul>

                <h4>"Privacy"</h4>
                <p>
                    "Your model is read and annotated in your browser and is never uploaded. The suggestions come "
                    "from a knowledge base downloaded with the page (see About). Only the searches you start send "
                    "anything: the search text, ChEBI ids or EC numbers, or paper ids for their titles."
                </p>
            </div>
        </Show>
    }
}

/// The keyboard shortcuts, shown with ? or the Shortcuts button.
#[component]
fn Shortcuts() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let rows: &[(&[&str], &str)] = &[
        (&["j", "Down"], "next row"),
        (&["k", "Up"], "previous row"),
        (&["n"], "next row with a suggestion to review"),
        (&["Enter", "o"], "open or close the row's details"),
        (&["a"], "accept the row's suggestion, then go to the next row"),
        (&["r"], "reject the row's suggestion, then go to the next row"),
        (&["/"], "search other databases for the row"),
        (&["1", "2", "3", "4"], "Molecules, Reactions, Compartments, Model"),
        (&["Ctrl+Z"], "undo"),
        (&["Ctrl+Y", "Ctrl+Shift+Z"], "redo"),
        (&["Esc"], "close Help, About, this, a pinned card, or the details"),
        (&["?"], "show or hide this list"),
    ];
    view! {
        <Show when=move || ctx.help.get()>
            <div class="shortcuts" role="dialog" aria-label="Keyboard shortcuts">
                <div class="shortcuts-head">
                    <h3>"Keyboard shortcuts"</h3>
                    <button class="link" on:click=move |_| ctx.help.set(false)>"Close"</button>
                </div>
                <table>
                    {rows
                        .iter()
                        .map(|(keys, what)| {
                            view! {
                                <tr>
                                    <td class="keys">
                                        {keys
                                            .iter()
                                            .enumerate()
                                            .map(|(i, k)| view! { {(i > 0).then_some(" or ")} <kbd>{*k}</kbd> })
                                            .collect_view()}
                                    </td>
                                    <td>{*what}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </table>
                <p class="muted">"Shortcuts do nothing while you type in a box."</p>
            </div>
        </Show>
    }
}

#[component]
fn Message() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    view! {
        <Show when=move || ctx.message.with(|m| m.is_some())>
            <div class="message" role="status">
                <span>{move || ctx.message.get().unwrap_or_default()}</span>
                <button class="link" on:click=move |_| ctx.message.set(None)>"Dismiss"</button>
            </div>
        </Show>
    }
}
