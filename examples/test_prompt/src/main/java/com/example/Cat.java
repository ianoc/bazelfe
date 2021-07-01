package com.example;

import com.animal.Animal;

public class Cat implements Animal {
    public String name = "Furry";

    public String feels_like() {
        return name;
    }
}