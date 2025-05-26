use anyhow::Result;
use anyhow::anyhow;
use crossterm::style::Stylize;
use im::Vector;

#[derive(Clone)]
pub struct MenuItem<T: Clone + Copy> {
    name: String,
    description: String,
    key: char,
    value: T,
}

impl<T: Clone + Copy> MenuItem<T> {
    pub fn new(value: T, name: &str, description: &str) -> MenuItem<T> {
        let key = name.to_lowercase().chars().next().unwrap();
        MenuItem {
            name: name.to_string(),
            description: description.to_string(),
            key,
            value,
        }
    }

    fn render(&self, first: bool, selected: bool) -> String {
        let mut text = format!(
            "{}({}){} ",
            if first { "" } else { " " },
            self.name.get(..1).unwrap(),
            self.name.get(1..).unwrap_or("")
        );
        if selected {
            text = text.dark_red().to_string();
        } else {
            text = text.dark_blue().to_string();
        }
        text
    }
}

pub struct Menu<T: Clone + Copy> {
    items: Vector<MenuItem<T>>,
    selected_index: usize,
}

impl<T: Clone + Copy> Menu<T> {
    pub fn new(items: Vector<MenuItem<T>>) -> Result<Menu<T>> {
        if items.is_empty() {
            Err(anyhow!("Menu: No menu items found"))
        } else {
            Ok(Menu {
                items,
                selected_index: 0,
            })
        }
    }

    pub fn select(&mut self, c: char) -> Option<T> {
        for (index, item) in self.items.iter().enumerate() {
            if item.key == c {
                self.selected_index = index;
                return Some(item.value);
            }
        }
        None
    }

    pub fn render(&self) -> String {
        let mut text = String::new();
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                text += "    ";
            }
            text += item.render(i == 0, i == self.selected_index).as_ref();
        }
        text
    }

    pub fn left(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = self.items.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    pub fn right(&mut self) {
        self.selected_index += 1;
        if self.selected_index >= self.items.len() {
            self.selected_index = 0
        }
    }

    pub fn description(&self) -> &str {
        self.items[self.selected_index].description.as_str()
    }

    pub fn value(&self) -> T {
        self.items[self.selected_index].value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use im::vector;

    #[derive(Copy, Clone)]
    enum MenuValue {
        Append,
        Reload,
        Quit,
    }

    #[test]
    fn test_menu_item() {
        let item = MenuItem::new(MenuValue::Append, "Append", "Add current date to the file.");
        assert_eq!(item.name, "Append");
        assert_eq!(item.description, "Add current date to the file.");
        assert_eq!(item.key, 'a');
        assert_eq!(item.render(true, true), "(A)ppend ".dark_red().to_string());
        assert_eq!(
            item.render(false, false),
            " (A)ppend ".dark_blue().to_string(),
        );
    }

    #[test]
    fn test_menu() {
        let menu_items = vector!(
            MenuItem::new(MenuValue::Append, "Append", "Add current date to the file."),
            MenuItem::new(MenuValue::Reload, "Reload", "Force reload of file."),
            MenuItem::new(MenuValue::Quit, "Quit", "Quit the program.")
        );
        let mut menu = Menu::new(menu_items.clone()).unwrap();
        for i in 0..menu_items.len() {
            assert_eq!(menu.selected_index, i);
            assert_eq!(
                menu.render(),
                format!(
                    "{}    {}    {}",
                    menu_items[0].render(true, menu.selected_index == 0),
                    menu_items[1].render(false, menu.selected_index == 1),
                    menu_items[2].render(false, menu.selected_index == 2)
                )
            );
            menu.right();
        }
        for i in (0..menu_items.len()).rev() {
            menu.left();
            assert_eq!(menu.selected_index, i);
        }
    }
}
