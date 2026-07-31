import { Component, computed, effect, inject, signal } from '@angular/core';
import { NavigationEnd, Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { MatMenuModule } from '@angular/material/menu';
import { MatSidenavModule } from '@angular/material/sidenav';
import { MatToolbarModule } from '@angular/material/toolbar';
import { filter } from 'rxjs/operators';

import { Api, EstadoLicencia } from './nucleo/api';
import { Sesion } from './nucleo/sesion';

interface Entrada {
  ruta: string;
  titulo: string;
  icono: string;
}

@Component({
  selector: 'app-root',
  imports: [
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    MatButtonModule,
    MatIconModule,
    MatListModule,
    MatMenuModule,
    MatSidenavModule,
    MatToolbarModule,
  ],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  sesion = inject(Sesion);
  private router = inject(Router);
  private api = inject(Api);

  /**
   * El aviso de licencia (#175). `null` mientras no se sabe.
   *
   * Es la razón de ser del módulo de licencia: **visibilidad**. Si esto no se
   * pinta, lo que queda es un endpoint que nadie mira y un licenciamiento que no
   * avisa de nada.
   *
   * Si la consulta FALLA se queda en `null` y no se pinta nada. No se inventa un
   * estado: un «licencia inválida» por un error de red sería una acusación
   * falsa, y un «licencia correcta» sería lo contrario. Sin dato, silencio.
   */
  readonly licencia = signal<EstadoLicencia | null>(null);

  readonly menu: Entrada[] = [
    { ruta: '/entregas', titulo: 'Entregas', icono: 'assignment_turned_in' },
    { ruta: '/notificaciones', titulo: 'Notificaciones', icono: 'notifications' },
    { ruta: '/estadisticas', titulo: 'Estadísticas', icono: 'insights' },
    { ruta: '/informes', titulo: 'Informes', icono: 'description' },
    { ruta: '/cronograma', titulo: 'Cronograma', icono: 'event_repeat' },
  ];

  /** El lateral arranca abierto solo si hay sitio. En un portátil de 13" abierto
   *  por defecto deja la tabla de entregas sin ancho para sus seis columnas. */
  readonly anchaLaPantalla = signal(window.innerWidth >= 1100);

  private readonly ruta = signal(this.router.url);
  readonly tituloActual = computed(
    () => this.menu.find((e) => this.ruta().startsWith(e.ruta))?.titulo ?? 'SIGER',
  );

  constructor() {
    this.router.events
      .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
      .subscribe((e) => this.ruta.set(e.urlAfterRedirects));

    // Se consulta al entrar y en cada navegación entre secciones, no una sola
    // vez: el servidor lee el archivo del disco sin caché a propósito, para que
    // una licencia sustituida en caliente se refleje sin reiniciar; cachearla
    // aquí desharía eso y la interfaz mentiría hasta el siguiente arranque.
    effect(() => {
      if (!this.sesion.dentro()) {
        this.licencia.set(null);
        return;
      }
      this.api.licencia().subscribe({
        next: (e) => this.licencia.set(e),
        error: () => this.licencia.set(null),
      });
    });
  }
}
